<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import FileTreeNode from './FileTreeNode.vue'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const props = defineProps({
  rootPath: { type: String, default: '' },
  rootName: { type: String, default: '' },
  nodes: { type: Array, default: () => [] },
  currentPath: { type: String, default: '' },
})
const emit = defineEmits(['open-file', 'hide-tree', 'layout-change'])

const settings = useSettingsStore()

/// 文件树滚动区元素：用于判断鼠标命中的行是否属于本树
const rootEl = ref(null)

// 展开状态集合：默认仅展开当前文件所在路径的各级父目录
const expanded = reactive(new Set())

// ==================== 展开状态持久化 ====================
// 按根目录分 key 保存，避免切换文件夹时状态互相污染
const LS_EXPANDED_PREFIX = 'mpe-tree-expanded:'
const lsKey = computed(() => LS_EXPANDED_PREFIX + (props.rootPath || ''))

/// 递归收集全部目录节点路径
function collectDirPaths(nodes, out = []) {
  for (const n of nodes || []) {
    if (!n.isDir) continue
    out.push(n.path)
    collectDirPaths(n.children, out)
  }
  return out
}

function persistExpanded() {
  try {
    localStorage.setItem(lsKey.value, JSON.stringify([...expanded]))
  } catch (e) {
    /* 隐私模式等场景下写入失败可忽略，仅丢失记忆不影响使用 */
  }
}

// 恢复上次展开状态（与当前文件祖先展开合并，取并集）
;(function restoreExpanded() {
  try {
    const raw = localStorage.getItem(lsKey.value)
    const arr = raw ? JSON.parse(raw) : []
    if (Array.isArray(arr)) arr.forEach(p => expanded.add(p))
  } catch (e) {
    /* 解析失败按默认状态处理 */
  }
})()

function expandAll() {
  collectDirPaths(props.nodes).forEach(p => expanded.add(p))
  persistExpanded()
}
function collapseAll() {
  expanded.clear()
  persistExpanded()
}

// ==================== 路径工具 ====================
const norm = p => String(p || '').replace(/\\/g, '/')
const rootNorm = computed(() => norm(props.rootPath).replace(/\/+$/, ''))

/// 节点绝对路径 → 相对根目录的路径（后端排版存档用的就是这个形态）
function relOf(path) {
  const p = norm(path)
  const root = rootNorm.value
  if (root && p.indexOf(root) === 0 && p.length > root.length) {
    return p.slice(root.length + 1)
  }
  return p
}

/// 相对路径的父目录（顶层为空串）
function parentRelOf(rel) {
  const i = rel.lastIndexOf('/')
  return i < 0 ? '' : rel.slice(0, i)
}

/// 把树按「显示顺序」拍平成相对路径列表：目录占自己的位置，其内容紧随其后。
///
/// 与后端 `collector::collect_with_config` 的遍历顺序一致——这份列表就是
/// 排版存档里的 `order`。之所以把目录也写进去，是因为文件与目录在树里
/// 是同一排的，只有目录也占位，才能表达「把 a.md 拖到 sub 上面」。
function flattenRels(nodes, out = []) {
  for (const n of nodes || []) {
    const rel = relOf(n.path)
    out.push(rel)
    if (n.isDir) flattenRels(n.children, out)
  }
  return out
}

/// 把「当前看不见、但存档里还有」的条目并回新顺序。
///
/// 被排除的目录不会向下展开（`scan_dir` 到它就停），所以 `flattenRels`
/// 拿到的树里根本没有它的后代。若把拍平结果整份覆盖 `order`，那些后代的
/// 排序就被无声抹掉了——用户取消排除后会看到目录内顺序退回默认，
/// 而他并没有动过那一段。这里把它们挂回最近的可见祖先之后，
/// 并保持存档里原有的相对次序。
///
/// 祖先已不存在的条目（文件被删了）直接丢弃，顺带清理存档。
function mergeHiddenOrder(moved, prevOrder) {
  const visible = new Set(moved)
  const buckets = new Map()
  for (const rel of prevOrder) {
    if (visible.has(rel)) continue
    // 最近的可见祖先：从最深的父级逐段往上截，第一个命中 moved 的就是
    let anc = ''
    for (let i = rel.lastIndexOf('/'); i > 0; i = rel.lastIndexOf('/', i - 1)) {
      if (visible.has(rel.slice(0, i))) {
        anc = rel.slice(0, i)
        break
      }
    }
    if (!anc) continue
    if (!buckets.has(anc)) buckets.set(anc, [])
    buckets.get(anc).push(rel)
  }
  if (!buckets.size) return moved
  const out = []
  for (const rel of moved) {
    out.push(rel)
    const extra = buckets.get(rel)
    if (extra) out.push(...extra)
  }
  return out
}

// ==================== 拖动排序（鼠标事件实现）====================
// 不用 HTML5 drag & drop 的原因：Tauri 开了 `dragDropEnabled`（默认 true）后，
// 原生拖放目标由 Rust 侧接管。wry 的 `dragging_entered` 在 handler 返回 true 时
// **不会**把事件交回 WKWebView（见 wry src/wkwebview/drag_drop.rs），而 Tauri
// 的 handler 恒返回 true —— 结果 WebKit 内部收不到任何拖入，HTML5 的
// dragstart/dragover/drop 全都不触发，树内排序与落点线因此同时失效。
//
// 关掉该开关不可行：Tauri 2.11 没有 `getPathForFile`，一旦关掉就再也拿不到
// 「拖文件进窗口」里的绝对路径，那个既有功能会废掉。
//
// 鼠标事件不受原生拖放接管影响，所以改用 mousedown → 阈值 → mousemove → mouseup
// 自行实现。附带好处是不依赖 dataTransfer 的跨平台差异。
//
// 只允许同层重排：把文件拖进别的目录属于「移动文件」，本工具不做文件系统操作，
// 那样会让「排序」悄悄变成一次不可撤销的 mv。
const drag = ref(null) // { rel, parent }
const dropTarget = ref(null) // { rel, pos: 'before' | 'after' }
/// 移动超过该像素数才算拖动，否则视为普通点击（避免点一下就被当成拖动）
const DRAG_THRESHOLD = 4

/// 排版的串行写入队列（见 persistLayout）
let layoutChain = Promise.resolve()

/// 当前鼠标位置命中的行元素：用 data-rel 反查，避免给每个节点挂事件监听
function rowAt(x, y) {
  const el = document.elementFromPoint(x, y)
  if (!el) return null
  const row = el.closest('[data-rel]')
  return row && rootEl.value && rootEl.value.contains(row) ? row : null
}

/// 根据鼠标在某行内的相对位置判断「插到它前面还是后面」
function dropPosFor(row, y) {
  const r = row.getBoundingClientRect()
  return y < r.top + r.height / 2 ? 'before' : 'after'
}

/// 命中行是否可作为落点：排除自己、自己的子树、跨层
function canDropOn(rel, row) {
  const d = drag.value
  if (!d) return false
  if (rel === d.rel) return false
  // 拖到自己子树里：块会被搬进自己内部，产出一个自相矛盾、无法解释的顺序
  if (rel.startsWith(d.rel + '/')) return false
  // 跨层不响应：拖进别的目录等于移动文件，本工具不做
  return parentRelOf(rel) === d.parent
}

let pending = null // { rel, parent, startX, startY, active }
/// 拖动结束后浏览器会补一个 click，若不拦截就会顺手把文件打开。
/// 用捕获阶段拦下这一次 click，保证「拖完不误触」。
///
/// 不能只靠「拦到就清」：浏览器并不保证每次拖动后都补发 click，
/// 标志一旦残留就会吞掉用户下一次无关的点击（表现为「点一下没反应」）。
/// 所以设置后只在当前这一轮事件循环内有效，用 setTimeout 兜底清除。
let swallowNextClick = false
let swallowTimer = 0

function armSwallowClick() {
  swallowNextClick = true
  clearTimeout(swallowTimer)
  swallowTimer = setTimeout(() => {
    swallowNextClick = false
  }, 0)
}

function onMouseDown(payload, e) {
  // 只响应左键；右键留给菜单，中键留给系统
  if (e.button !== 0) return
  const rel = relOf(payload.path)
  pending = { rel, parent: parentRelOf(rel), startX: e.clientX, startY: e.clientY, active: false }
}

function onMouseMove(e) {
  if (!pending) return
  if (!pending.active) {
    const dx = Math.abs(e.clientX - pending.startX)
    const dy = Math.abs(e.clientY - pending.startY)
    // 未过阈值：仍在「可能是点击」的区间，不进入拖动状态
    if (dx < DRAG_THRESHOLD && dy < DRAG_THRESHOLD) return
    pending.active = true
    drag.value = { rel: pending.rel, parent: pending.parent }
  }
  const row = rowAt(e.clientX, e.clientY)
  if (!row) {
    if (dropTarget.value) dropTarget.value = null
    return
  }
  const rel = row.dataset.rel
  if (!canDropOn(rel, row)) {
    if (dropTarget.value) dropTarget.value = null
    return
  }
  const pos = dropPosFor(row, e.clientY)
  if (!dropTarget.value || dropTarget.value.rel !== rel || dropTarget.value.pos !== pos) {
    dropTarget.value = { rel, pos }
  }
}

async function onMouseUp(e) {
  const wasActive = pending && pending.active
  const d = drag.value
  const target = dropTarget.value
  pending = null
  drag.value = null
  dropTarget.value = null

  // 没进入拖动（只是点击），或没有有效落点：什么都不做
  if (!wasActive || !d || !target) return
  // 拖动过：抑制紧随其后的 click，避免把文件打开
  armSwallowClick()
  // 松手位置已不在目标行上（例如在行间空白处松开）：用落点提示为准，
  // 因为提示线就是用户看到的「将插到这里」
  const moved = moveBlock(flattenRels(props.nodes), d.rel, target.rel, target.pos)
  try {
    // 传入函数而不是数组：合并要用的旧 order 必须在队列内部、紧挨着写入之前
    // 读，否则与右键排除并发时读到的是被覆盖前的值
    await persistLayout({ order: prev => mergeHiddenOrder(moved, prev) })
  } catch (_) {
    /* 存档失败时后端树不变，「看到的即导出的」仍然成立 */
  }
}

/// 捕获阶段拦截拖动后的那次 click。
///
/// 必须用捕获（第三个参数 true）：行的 @click 绑在目标元素上，
/// 冒泡阶段才生效，等到那时文件已经打开了。
function onClickCapture(e) {
  if (!swallowNextClick) return
  swallowNextClick = false
  clearTimeout(swallowTimer)
  e.stopPropagation()
  e.preventDefault()
}

/// Esc 取消拖动：清掉状态即可，不留任何存档痕迹
function onKeydown(e) {
  if (e.key === 'Escape' && pending) {
    pending = null
    drag.value = null
    dropTarget.value = null
  }
  if (e.key === 'Escape') closeCtxMenu()
}

/// 把 `from` 连同其整棵子树搬到 `to` 之前/之后，返回新的拍平顺序。
///
/// 子树必须整体搬：只移动目录自身会让它和内容的位置关系错乱，
/// 而 `order` 里目录的位置决定了它那一段内容落在导出的哪一处。
function moveBlock(rels, from, to, pos) {
  const startOf = rel => rels.findIndex(r => r === rel || r.startsWith(rel + '/'))
  const start = startOf(from)
  if (start < 0) return rels
  // 块 = 自身 + 所有以它为前缀的后代（拍平顺序里是连续的一段）
  let end = start + 1
  while (end < rels.length && rels[end].startsWith(from + '/')) end += 1
  const block = rels.slice(start, end)
  const rest = rels.slice(0, start).concat(rels.slice(end))

  let at = rest.findIndex(r => r === to)
  if (at < 0) return rels
  if (pos === 'after') {
    // 落在目标之后：把目标整块跳过，避免插进它的子节点中间
    at += 1
    while (at < rest.length && rest[at].startsWith(to + '/')) at += 1
  }
  return rest.slice(0, at).concat(block, rest.slice(at))
}

/// 写回排版存档，并请后端按新规则重算树。
///
/// 刻意不在前端就地改数组：树长什么样由后端的 `scan_tree` 按同一套规则
/// 算出来，导出也用那套规则。前端只提交「我要的顺序/排除」，看到的结果
/// 与磁盘上的真实状态因此不会漂移。
///
/// `patch` 的字段可以是新值，也可以是 `prev => newValue`：后者让「依赖旧值的
/// 计算」（如保留被排除目录的内部顺序）在队列内部完成，避免并发读到过期快照。
///
/// 返回值可在 UI 上等（例如排除后要提示「已更新」），内部串行排队。
function persistLayout(patch) {
  const job = layoutChain.then(() => doPersistLayout(patch))
  // 失败不阻塞队列：后续操作应继续尝试，而不是被前一次错误卡死
  layoutChain = job.catch(() => {})
  return job
}

async function doPersistLayout(patch) {
  const cur = await readLayout()
  const pick = (v, fallback) =>
    v === undefined ? fallback : typeof v === 'function' ? v(fallback) : v
  const next = {
    order: pick(patch.order, cur.order),
    exclude: pick(patch.exclude, cur.exclude),
  }
  await invoke('save_folder_layout', { folder: props.rootPath, layout: next })
  const tree = await invoke('open_folder', { path: props.rootPath })
  emit('layout-change', tree)
}

async function readLayout() {
  try {
    const l = await invoke('get_folder_layout', { folder: props.rootPath })
    return { order: l?.order || [], exclude: l?.exclude || [] }
  } catch (_) {
    return { order: [], exclude: [] }
  }
}

/// 恢复默认顺序：清掉 order，保留排除项（用户可能只想重排，不想解排除）
async function resetOrder() {
  await persistLayout({ order: [] })
  closeCtxMenu()
}

// ==================== 右键菜单：复制路径 / 排除 ====================
// 单一菜单实例：所有节点右键仅冒泡位置信息，由本组件统一渲染，避免递归组件内多处挂载。
const ctxMenu = reactive({
  visible: false,
  x: 0,
  y: 0,
  path: '',
  name: '',
  repoRel: '',
  excluded: false,
  isDir: false,
  /// 该文件的渲染器 id（目录为空串）
  renderer: '',
})

// 菜单按最大项数估算高度，为保证空间稳定不随异步结果跳动，统一按最大高度预留
// 8 项：复制相对路径 / 复制仓库路径 / 导出 MD / 导出 HTML / 导出 DOCX / 导出 PDF / 排除 / 恢复顺序
const CTX_MENU_MAX_H = 300
const CTX_MENU_W = 240

/// 右键导出：打开系统保存对话框，再让后端把「指定路径」的文件导出过去。
///
/// 与工具栏导出共用后端同一套导出实现（`export_markdown` / `export_html` …），
/// 唯一差别是显式带上 `path`——这样右键导出不必先把目标文件打开，
/// 否则用户为了导出某个 dbml 得先点开它、丢掉当前正在读的文档。
async function exportNode(format) {
  const p = ctxMenu.path
  const base = ctxMenu.name.replace(/\.[^.]+$/, '') || 'export'
  closeCtxMenu()
  const nameMap = { html: 'HTML 文档', pdf: 'PDF 文档', docx: 'Word 文档', markdown: 'Markdown 文档' }
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: nameMap[format], extensions: format === 'markdown' ? ['md'] : [format] }],
        defaultPath: base + (format === 'markdown' ? '.md' : '.' + format),
        title: 'Export ' + format.toUpperCase(),
      },
    })
    if (!d) return
    if (format === 'markdown') {
      await invoke('export_markdown', { dst: d, path: p })
    } else if (format === 'docx') {
      await invoke('export_docx', { dst: d, path: p })
    } else {
      // HTML / PDF 需要模版与主题：用户期望的显然是「我当前在看的那一套」，
      // 所以带上设置里的当前值，而不是后端默认值
      await invoke('export_' + format, {
        dst: d,
        path: p,
        fullWidth: settings.fullWidth,
        theme: settings.theme,
        templateId: settings.templateId,
      })
    }
  } catch (_) {
    /* 导出失败由后端报错；这里不额外弹窗，避免菜单关闭后再弹一层 */
  }
}

/// 是否显示「导出 Markdown」：Markdown 原样复制意义不大，
/// 真正的用途是 DBML → 数据字典 md，但仍对全部文件开放以免用户困惑
/// （统一入口，行为一致）。DOCX 项按渲染器是否支持决定。
function canExportDocx() {
  if (ctxMenu.isDir) return false
  const r = settings.rendererList.find(x => x.id === ctxMenu.renderer)
  // 清单未拉取时保守显示：后端会拒绝并报错，但至少不会「本该有却没入口」
  return r ? r.supportsDocx : true
}

async function onNodeContextMenu(info) {
  // 预留菜单尺寸做边缘回退，避免贴边溢出窗口
  ctxMenu.path = info.path
  ctxMenu.name = norm(info.path).split('/').pop() || info.path
  ctxMenu.repoRel = ''
  ctxMenu.excluded = !!info.excluded
  ctxMenu.isDir = !!info.isDir
  ctxMenu.renderer = info.renderer || ''
  ctxMenu.x = Math.min(info.x, window.innerWidth - CTX_MENU_W)
  ctxMenu.y = Math.min(info.y, window.innerHeight - CTX_MENU_MAX_H)
  ctxMenu.visible = true
  // 渲染器清单要用来判断「支持哪些导出格式」，首次右键时兜底拉一次
  if (settings.rendererList.length === 0) settings.loadRenderTypes()

  // 异步补充仓库相对路径：不在任何 git 仓库内时 invoke 返回 null，届时该项不渲染
  const target = info.path
  try {
    const rel = await invoke('repo_relative_path', { path: target })
    // 期间若菜单已关闭或已切换到别的节点，丢弃这次结果
    if (rel && ctxMenu.visible && ctxMenu.path === target) ctxMenu.repoRel = rel
  } catch (_) {
    /* 后端命令不可用（如浏览器调试模式）时静默降级，仅保留相对路径项 */
  }
}

function closeCtxMenu() {
  ctxMenu.visible = false
}

// 相对路径：剥离根目录前缀后，补上被打开文件夹自身名称作为首段
// （后端 node.path 已统一为 / 分隔的绝对路径）。
// 例：打开 docs 文件夹 → docs/guide/setup.md；无法剥离时（异常情况）退回节点名
const ctxRelPath = computed(() => {
  const root = rootNorm.value
  const p = norm(ctxMenu.path)
  if (root && p.indexOf(root) === 0 && p.length > root.length) {
    const base = props.rootName || root.split('/').pop() || ''
    return base ? base + '/' + p.slice(root.length + 1) : p.slice(root.length + 1)
  }
  return ctxMenu.name
})

// 复制到剪贴板：优先 Clipboard API，失败时退回 execCommand 兜底
async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text)
  } catch {
    const ta = document.createElement('textarea')
    ta.value = text
    ta.style.position = 'fixed'
    ta.style.opacity = '0'
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
  }
}

async function copyRelPath() {
  await copyText(ctxRelPath.value)
  closeCtxMenu()
}

// 复制仓库相对路径（含仓库名，形如 md2x/crates/md2x-gui/src/lib.rs）。
async function copyRepoRelPath() {
  if (!ctxMenu.repoRel) return
  try {
    await invoke('copy_text', { text: ctxMenu.repoRel })
  } catch (_) {
    /* 后端不可用时退回前端兜底复制 */
    await copyText(ctxMenu.repoRel)
  }
  closeCtxMenu()
}

/// 排除 / 取消排除当前节点。
///
/// 排除目录时只记目录本身：后端 `is_excluded` 按分隔符边界匹配后代，
/// 不必把几十个子文件逐个写进列表，取消时也只需删一项。
async function toggleExclude() {
  const rel = relOf(ctxMenu.path)
  const excluded = ctxMenu.excluded
  closeCtxMenu()
  await persistLayout({
    exclude: prev => {
      const set = new Set(prev)
      if (excluded) {
        set.delete(rel)
      } else {
        // 父目录已被排除时，再排除子节点没有意义（父级已经整棵排除）
        for (const e of set) {
          if (rel.startsWith(e + '/')) return prev
        }
        set.add(rel)
      }
      return [...set]
    },
  })
}

// 菜单打开期间监听全局点击/滚动/Escape，任意一处即关闭
function handleGlobalClose() {
  if (ctxMenu.visible) closeCtxMenu()
}
onMounted(() => {
  window.addEventListener('click', handleGlobalClose)
  window.addEventListener('blur', handleGlobalClose)
  window.addEventListener('keydown', onKeydown)
  // 拖动监听必须挂 window：鼠标很容易在拖动途中移出树甚至移出窗口，
  // 挂在树上会漏掉 mouseup，导致拖动状态卡住
  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
  // 捕获阶段拦拖动后的误触点击（见 onClickCapture 注释）
  window.addEventListener('click', onClickCapture, true)
})
onBeforeUnmount(() => {
  window.removeEventListener('click', handleGlobalClose)
  window.removeEventListener('blur', handleGlobalClose)
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
  window.removeEventListener('click', onClickCapture, true)
})

/// 拖动进行中给 body 加全局样式：禁止选中文字、统一光标
watch(drag, d => {
  const on = !!d
  document.body.style.userSelect = on ? 'none' : ''
  document.body.style.cursor = on ? 'grabbing' : ''
})

const activeNorm = computed(() => norm(props.currentPath))

// 若当前文件位于树内某目录下，确保其父级目录展开（文件切换 / 前进后退 / 启动定位）
function expandAncestors(nodes, target) {
  if (!target) return
  for (const n of nodes) {
    if (!n.isDir) continue
    const isAncestor =
      target.indexOf(n.path) === 0 &&
      (target.length === n.path.length || target.charAt(n.path.length) === '/')
    if (isAncestor) {
      expanded.add(n.path)
      expandAncestors(n.children || [], target)
    }
  }
}
expandAncestors(props.nodes, activeNorm.value)
watch(() => activeNorm.value, v => expandAncestors(props.nodes, v))

function toggle(path) {
  if (expanded.has(path)) expanded.delete(path)
  else expanded.add(path)
  persistExpanded()
}
function openFile(path) {
  emit('open-file', path)
}
</script>

<template>
  <aside
    class="flex flex-col flex-shrink-0 h-full overflow-hidden select-none"
    :style="{
      width: '264px',
      background: 'var(--surface)',
      borderRight: '1px solid var(--border)',
    }"
  >
    <!-- 根目录标题 + 隐藏文件树（仅折叠面板，可再显示） -->
    <div
      class="flex items-center gap-2 h-11 px-3 flex-shrink-0"
      :style="{ borderBottom: '1px solid var(--border)' }"
    >
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 flex-shrink-0" style="color: var(--amber)">
        <path d="M1.5 4.5a1 1 0 0 1 1-1h3.2l1.6 2h5.2a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-7z" />
        <path d="M9.5 7h4" />
      </svg>
      <span
        class="flex-1 truncate text-[12.5px] font-semibold"
        :style="{ color: 'var(--text)' }"
        :title="rootPath"
      >{{ rootName || rootPath }}</span>

      <!-- 批量展开 / 收起：作用于整棵文件树 -->
      <button
        class="flex items-center justify-center w-6 h-6 rounded-md cursor-pointer border-none transition-colors duration-100 hover:opacity-80"
        :style="{ color: 'var(--text-muted)', background: 'transparent' }"
        :title="t('treeExpandAll', settings.lang)"
        @click.stop="expandAll"
      >
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
          <path d="M5.5 6.5L8 4l2.5 2.5" />
          <path d="M5.5 12.5L8 10l2.5 2.5" />
          <path d="M3 6.5h10" />
          <path d="M3 12.5h10" />
        </svg>
      </button>
      <button
        class="flex items-center justify-center w-6 h-6 rounded-md cursor-pointer border-none transition-colors duration-100 hover:opacity-80"
        :style="{ color: 'var(--text-muted)', background: 'transparent' }"
        :title="t('treeCollapseAll', settings.lang)"
        @click.stop="collapseAll"
      >
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
          <path d="M5.5 4L8 6.5 10.5 4" />
          <path d="M5.5 12.5L8 10l2.5 2.5" />
          <path d="M3 6.5h10" />
          <path d="M3 12.5h10" />
        </svg>
      </button>
      <button
        class="flex items-center justify-center w-6 h-6 rounded-md cursor-pointer border-none transition-colors duration-100 hover:opacity-80"
        :style="{ color: 'var(--text-muted)', background: 'transparent' }"
        :title="t('fileTreeHide', settings.lang)"
        @click="emit('hide-tree')"
      >
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>

    <!-- 拖动提示：一次说清「拖动能改什么」，省得用户以为能拖进别的目录 -->
    <div
      class="px-3 py-1 text-[11px] flex-shrink-0"
      :style="{ color: 'var(--text-dim)' }"
    >{{ t('treeDragTip', settings.lang) }}</div>

    <!-- 文件树滚动区 -->
    <div ref="rootEl" class="flex-1 overflow-y-auto py-1.5">
      <template v-if="nodes.length">
        <FileTreeNode
          v-for="node in nodes"
          :key="node.path"
          :node="node"
          :depth="0"
          :expanded="expanded"
          :active-path="activeNorm"
          :dragging-rel="drag ? drag.rel : ''"
          :drop-target="dropTarget"
          :rel-of="relOf"
          @open="openFile"
          @toggle="toggle"
          @contextmenu="onNodeContextMenu"
          @drag-start="onMouseDown"
        />
      </template>
      <div
        v-else
        class="text-center text-xs mt-8 px-6 leading-relaxed"
        :style="{ color: 'var(--text-dim)' }"
      >{{ t('treeNoRenderable', settings.lang) }}</div>
    </div>

    <!-- 右键菜单：路径复制（相对当前文件夹 / 相对 git 仓库）+ 排除 -->
    <Teleport to="body">
      <div
        v-if="ctxMenu.visible"
        class="fixed z-[1000] min-w-[180px] rounded-md py-1 shadow-lg"
        :style="{
          left: ctxMenu.x + 'px',
          top: ctxMenu.y + 'px',
          background: 'var(--surface)',
          border: '1px solid var(--border)',
        }"
        @contextmenu.prevent
      >
        <button
          class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
          :style="{ background: 'transparent', color: 'var(--text)' }"
          :title="ctxRelPath"
          @click.stop="copyRelPath"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
            <rect x="5.5" y="5.5" width="8" height="8" rx="1" />
            <path d="M10.5 5.5V3a1 1 0 0 0-1-1h-6a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.5" />
          </svg>
          {{ t('treeCopyRelPath', settings.lang) }}
        </button>
        <!-- 仓库相对路径：仅当节点位于 git 仓库内时出现（后端返回 null 则不渲染） -->
        <button
          v-if="ctxMenu.repoRel"
          class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
          :style="{ background: 'transparent', color: 'var(--text)' }"
          :title="ctxMenu.repoRel"
          @click.stop="copyRepoRelPath"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
            <rect x="5.5" y="5.5" width="9" height="9" rx="1.5" />
            <path d="M10.5 5.5v-2a1.5 1.5 0 0 0-1.5-1.5H4a1.5 1.5 0 0 0-1.5 1.5v5A1.5 1.5 0 0 0 4 10h1.5" />
          </svg>
          {{ t('ctxCopyRepoRelPath', settings.lang) }}
        </button>

        <div class="my-1" :style="{ borderTop: '1px solid var(--border)' }" />

        <!-- 导出：文件节点才出现。DBML 的「导出 Markdown」即数据字典式 md -->
        <template v-if="!ctxMenu.isDir">
          <button
            class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
            :style="{ background: 'transparent', color: 'var(--text)' }"
            @click.stop="exportNode('markdown')"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
              <path d="M2 3.5h3.4l1 1.2h6.6v7.8H2z" />
              <path d="M6 9.2h4M8 7.4v3.6" />
            </svg>
            {{ t('ctxExportMd', settings.lang) }}
          </button>
          <button
            class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
            :style="{ background: 'transparent', color: 'var(--text)' }"
            @click.stop="exportNode('html')"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
              <path d="M5.5 6L3 8l2.5 2M10.5 6L13 8l-2.5 2M9 4.5l-2 7" />
            </svg>
            {{ t('ctxExportHtml', settings.lang) }}
          </button>
          <button
            class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
            :style="{ background: 'transparent', color: 'var(--text)' }"
            @click.stop="exportNode('pdf')"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
              <path d="M4 1.5h5l3 3v10H4z" />
              <path d="M9 1.5v3h3" />
              <path d="M6 11V8.5h1.2a1 1 0 0 1 0 2H6" />
            </svg>
            {{ t('ctxExportPdf', settings.lang) }}
          </button>
          <button
            v-if="canExportDocx()"
            class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
            :style="{ background: 'transparent', color: 'var(--text)' }"
            @click.stop="exportNode('docx')"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
              <path d="M4 1.5h5l3 3v10H4z" />
              <path d="M9 1.5v3h3" />
              <path d="M5.8 8l.9 4 1.3-2.6L9.3 12l.9-4" />
            </svg>
            {{ t('ctxExportDocx', settings.lang) }}
          </button>
        </template>

        <div v-if="!ctxMenu.isDir" class="my-1" :style="{ borderTop: '1px solid var(--border)' }" />

        <!-- 排除：不导出。目录被排除时其内容一并排除，故不做逐级勾选 -->
        <button
          class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
          :style="{ background: 'transparent', color: 'var(--text)' }"
          @click.stop="toggleExclude"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
            <template v-if="ctxMenu.excluded">
              <path d="M2.5 8a5.5 5.5 0 0 1 9.4-3.9" />
              <path d="M13.5 8a5.5 5.5 0 0 1-9.4 3.9" />
              <path d="M2.5 3v3h3" />
              <path d="M13.5 13v-3h-3" />
            </template>
            <template v-else>
              <circle cx="8" cy="8" r="5.5" />
              <path d="M4.5 11.5l7-7" />
            </template>
          </svg>
          {{ ctxMenu.excluded ? t('treeInclude', settings.lang) : t('treeExclude', settings.lang) }}
        </button>

        <button
          class="flex items-center gap-2 w-full px-3 py-1.5 text-[12.5px] border-none cursor-pointer text-left transition-colors duration-100 hover:opacity-80"
          :style="{ background: 'transparent', color: 'var(--text)' }"
          @click.stop="resetOrder"
        >
          <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5 flex-shrink-0" style="color: var(--text-muted)">
            <path d="M3 8a5 5 0 1 1 1.5 3.5" />
            <path d="M3 4v4h4" />
          </svg>
          {{ t('treeResetLayout', settings.lang) }}
        </button>
      </div>
    </Teleport>
  </aside>
</template>