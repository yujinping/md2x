<script setup>
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from './stores/settings.js'
import { t } from './i18n/index.js'
import Toolbar from './components/Toolbar.vue'
import Welcome from './components/Welcome.vue'
import StatusBar from './components/StatusBar.vue'
import AboutDialog from './components/AboutDialog.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import PdfOverlay from './components/PdfOverlay.vue'
import FileTree from './components/FileTree.vue'

const settings = useSettingsStore()
const previewState = ref(null)
const hasFile = ref(false)
const fileName = ref('')
const statusText = ref('')
const statusType = ref('')
const isPdfView = ref(false)
const isGeneratingPdf = ref(false)
// 当前打开文件的完整路径，用于把相对链接解析为绝对路径
const currentPath = ref('')

// ===== 文件夹 / 文件树视图 =====
const isFolderView = ref(false)
// 文件树是否可见（目录式折叠：关闭只是隐藏，可随时再显示）
const fileTreeVisible = ref(true)
// 进入全宽阅读前文件树的可见状态，退出时恢复
const savedTreeVisible = ref(true)
const folderPath = ref('')
const folderTree = ref(null) // TreeNode[] | null
const folderName = computed(() => {
  const p = (folderPath.value || '').replace(/\\/g, '/').replace(/\/+$/, '')
  const i = p.lastIndexOf('/')
  return i >= 0 ? p.slice(i + 1) : p || folderPath.value
})

function setStatus(msg, type) {
  statusText.value = t(msg, settings.lang)
  statusType.value = type || ''
}

// 导航历史栈（绝对路径），供工具栏前进/后退使用
const history = ref([])
const historyIndex = ref(-1)
const canGoBack = computed(() => historyIndex.value > 0)
const canGoForward = computed(
  () => historyIndex.value >= 0 && historyIndex.value < history.value.length - 1,
)

/// 把一次导航压入历史栈：与当前相同则不重复；处于历史中间则截断前进分支
function pushHistory(path) {
  if (historyIndex.value >= 0 && history.value[historyIndex.value] === path) return
  if (historyIndex.value < history.value.length - 1) {
    history.value = history.value.slice(0, historyIndex.value + 1)
  }
  history.value.push(path)
  historyIndex.value = history.value.length - 1
}

/// 打开文件并渲染预览（不改动历史栈），前进/后退也走这里
async function openFileAndRender(path) {
  currentPath.value = path
  previewState.value = null
  setStatus('statusGenerating', 'busy')
  try {
    await invoke('set_file', { path })
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  await loadPreview()
}

/// 打开文件并记入历史（链接点击 / 打开文件 / 拖拽均走此入口）
async function openMdFile(path) {
  pushHistory(path)
  await openFileAndRender(path)
}

/// 后退：回到历史中的上一份文件
async function goBack() {
  if (!canGoBack.value) return
  historyIndex.value -= 1
  await openFileAndRender(history.value[historyIndex.value])
}

/// 前进：回到历史中的下一份文件
async function goForward() {
  if (!canGoForward.value) return
  historyIndex.value += 1
  await openFileAndRender(history.value[historyIndex.value])
}

async function loadPreview() {
  let tmp
  try {
    tmp = await invoke('get_html')
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  isPdfView.value = false
  hasFile.value = true

  // Read HTML into srcdoc
  let html = ''
  try {
    let off = 0, done = false
    while (!done) {
      const [chunk, last] = await invoke('read_file_chunk', { path: tmp, offset: off })
      html += chunk
      off += 524288
      done = last
    }
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  previewState.value = injectLinkHandler(applyFullWidth(html))

  try {
    const n = await invoke('get_file_name')
    fileName.value = n || ''
  } catch (_) {}

  setStatus('statusPreviewReady', 'ready')
}

/// 向内嵌预览 HTML 注入点击拦截脚本：
/// 点击指向 .md 的内部链接时，阻止 iframe 自行跳转（否则会嵌套空页面），
/// 改为通知父窗口在应用内打开对应文件。
function injectLinkHandler(html) {
  const open = '<' + 'script>'
  const close = '<' + '/script>'
  const code = `
(function () {
  document.addEventListener('click', function (e) {
    var a = e.target && e.target.closest ? e.target.closest('a') : null;
    if (!a) return;
    var href = a.getAttribute && a.getAttribute('href');
    if (!href) return;
    // 仅拦截指向 .md 的内部链接（相对路径或 file://），锚点与外链放行
    if (/\\.md([?#]|$)/i.test(href)) {
      e.preventDefault();
      if (window.parent && window.parent !== window) {
        window.parent.postMessage({ type: 'md2x-link', href: href }, '*');
      }
    }
  }, true);
})();
`
  const script = open + code + close
  if (html.indexOf('</body>') !== -1) {
    return html.replace('</body>', script + '</body>')
  }
  return html + script
}

/// 纯前端全宽兜底：当 settings.fullWidth 开启时，直接在 HTML 的 <head> 注入
/// 全宽 CSS，彻底绕开后端命令参数传递（规避 Tauri v2 参数大小写 / reject 风险）。
/// 与后端 render 注入的 CSS 完全等价，重复注入无害。
function applyFullWidth(html) {
  if (!settings.fullWidth) return html
  const style = `<style id="md2x-fullwidth">\
:root{--layout-max-width:none!important;--content-max-width:100%!important}\
.sidebar,.sidebar-resizer,.sidebar-toggle{display:none!important}\
.layout{max-width:none!important}\
.main-content{margin-left:0!important;padding-left:24px!important;padding-right:24px!important}\
.markdown-body{max-width:100%!important}\
</style>`
  if (html.indexOf('</head>') !== -1) {
    return html.replace('</head>', style + '</head>')
  }
  return html + style
}

/// 处理 iframe 转发来的内部链接点击：解析为绝对路径并在应用内打开
async function resolveAndOpen(href) {
  // 以当前打开文件所在目录为基准解析相对链接；
  // 若前端记录的当前路径为空（极端时序），回退到 Rust 端权威路径
  let basePath = currentPath.value
  if (!basePath) {
    try { basePath = (await invoke('get_file_path')) || '' } catch (_) {}
  }
  if (!basePath) return

  let target
  if (href.startsWith('file://')) {
    target = decodeURIComponent(href.replace(/^file:\/\//, ''))
    if (!target.startsWith('/') && !target.startsWith('//')) target = '/' + target
  } else {
    // 相对路径：以当前打开文件所在目录为基准解析（支持 ./ ../ 与 %20 等编码）
    const dir = basePath.replace(/[\\/][^\\/]*$/, '')
    const base = 'file://' + (dir.startsWith('/') ? '' : '/') + dir + '/'
    try {
      // new URL 会把中文等非 ASCII 字符做百分号编码，pathname 不是真实文件路径，
      // 必须 decodeURIComponent 还原成文件系统真实路径，否则 set_file 找不到文件
      target = decodeURIComponent(new URL(href, base).pathname)
    } catch (err) {
      return
    }
  }
  if (!target || !/\.md$/i.test(target)) return
  // 在应用内以 HTML 方式打开该 .md（openMdFile 会重新渲染预览）
  openMdFile(target)
}

function onIframeMessage(e) {
  const data = e.data
  if (!data || data.type !== 'md2x-link') return
  resolveAndOpen(data.href)
}

async function onExportPdf() {
  if (previewState.value && isPdfView.value) {
    showPdfViewer(previewState.value)
    return
  }

  // 先显示遮罩层，将 PDF 生成推到下一个 macrotask
  // 确保浏览器有时间渲染遮罩层
  isGeneratingPdf.value = true
  await new Promise(resolve => setTimeout(resolve, 50))

  setStatus('statusGeneratingPdf', 'busy')
  try {
    const r = await invoke('preview_pdf', { fullWidth: settings.fullWidth })
    previewState.value = r
    showPdfViewer(r)
    setStatus('statusPdfReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  } finally {
    isGeneratingPdf.value = false
  }
}

function showPdfViewer(r) {
  isPdfView.value = true
  fileName.value = r.file_name || ''
}

async function onSavePdf() {
  if (!previewState.value || typeof previewState.value === 'string') {
    setStatus('statusNoPdf', 'error')
    return
  }
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: 'PDF 文档', extensions: ['pdf'] }],
        defaultPath: previewState.value.file_name,
        title: 'Save PDF'
      }
    })
    if (!d) return
    setStatus('statusSaving', 'busy')
    await invoke('save_pdf_as', { src: previewState.value.temp_path, dst: d })
    setStatus('statusPdfSaved', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

async function onExportDoc(format) {
  const ext = format
  const nameMap = { html: 'HTML 文档', pdf: 'PDF 文档', docx: 'Word 文档' }
  const base = (fileName.value || 'document').replace(/\.md$/i, '')
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: nameMap[format], extensions: [ext] }],
        defaultPath: base + '.' + ext,
        title: 'Export ' + ext.toUpperCase()
      }
    })
    if (!d) return
    setStatus('statusExporting', 'busy')
    if (format === 'docx') {
      await invoke('export_docx', { dst: d })
    } else {
      await invoke('export_' + format, { dst: d, fullWidth: settings.fullWidth })
    }
    setStatus('statusExportReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

function toggleFullWidth() {
  const next = !settings.fullWidth
  settings.setFullWidth(next)
  const hasFolder = isFolderView.value && !!folderTree.value
  if (next) {
    // 全宽阅读模式 = 只看正文：隐藏文档目录（CSS 注入）并同时收起文件夹树
    savedTreeVisible.value = fileTreeVisible.value
    if (hasFolder) fileTreeVisible.value = false
  } else if (hasFolder) {
    fileTreeVisible.value = savedTreeVisible.value
  }
  // 重新渲染预览以应用新的宽度模式（PDF 视图则切回 HTML 预览）
  if (isPdfView.value) isPdfView.value = false
  loadPreview()
}

function showHtmlPreview() {
  isPdfView.value = false
  // 从 PDF 视图返回时重新加载 HTML 预览
  loadPreview()
}

// ===== 文件夹视图操作 =====

/// 卸载当前预览的文件（前端状态 + 后端当前文件），回到默认欢迎视图
async function resetCurrentFile() {
  previewState.value = null
  hasFile.value = false
  fileName.value = ''
  isPdfView.value = false
  currentPath.value = ''
  history.value = []
  historyIndex.value = -1
  setStatus('statusReady', 'ready')
  try {
    await invoke('clear_file')
  } catch (_) {}
}

/// 从文件树顶层找出根目录的 README.md（优先精确匹配，其次忽略大小写）
function findRootReadme(tree) {
  const root = Array.isArray(tree) ? tree : []
  for (const n of root) {
    if (!n.isDir && n.name === 'README.md') return n.path
  }
  for (const n of root) {
    if (!n.isDir && n.name.toLowerCase() === 'readme.md') return n.path
  }
  return null
}

/// 打开文件夹：请求后端扫描并展示文件树
/// 若此前已打开过文件夹，先重置旧视图再打开新文件夹（等效于自动关闭前一个），
/// 保证文件树以全新状态渲染、不残留旧目录的展开/高亮状态
/// opts.autoReadme=false：仅围绕当前文件展开树时（如“文件夹视图”默认启动），不自动切到 README
async function openFolder(path, opts = {}) {
  if (!path) return
  const norm = s => String(s || '').replace(/\\/g, '/').replace(/\/+$/, '')
  const target = norm(path)
  const prevFolderOpen = isFolderView.value || !!folderTree.value
  if (prevFolderOpen) {
    // 自动关闭前一个文件夹：先清空旧文件夹视图
    isFolderView.value = false
    folderTree.value = null
    folderPath.value = ''
    // 若正在预览的文件不在新文件夹内（属于旧文件夹），随切换一并卸载
    const cur = norm(currentPath.value)
    const inside = cur && (cur === target || cur.startsWith(target + '/'))
    if (currentPath.value && !inside) await resetCurrentFile()
  }
  try {
    const tree = await invoke('open_folder', { path })
    folderPath.value = path
    folderTree.value = tree
    isFolderView.value = true
    // 全宽阅读模式下打开新文件夹也不展开文件树，保持只看正文
    fileTreeVisible.value = !settings.fullWidth
    // 打开文件夹时：若根目录存在 README.md 且当前未打开任何文件，则自动渲染它；
    // 否则保持现有欢迎介绍页
    if (opts.autoReadme !== false && !hasFile.value) {
      const readme = findRootReadme(tree)
      if (readme) await openMdFile(readme)
    }
  } catch (e) {
    setStatus(e.toString(), 'error')
  }
}

/// 隐藏文件树（仅折叠面板，文件夹保持打开，可再显示）
function hideFileTree() {
  fileTreeVisible.value = false
}

/// 切换文件树显示/隐藏（工具栏按钮）
function toggleFileTree() {
  if (!isFolderView.value && !folderTree.value) return
  fileTreeVisible.value = !fileTreeVisible.value
}

/// 关闭文件夹：卸载文件夹内打开的预览文件，回到默认欢迎视图（初始状态）
async function closeFolder() {
  const hadFolder = isFolderView.value || !!folderTree.value
  isFolderView.value = false
  folderTree.value = null
  folderPath.value = ''
  fileTreeVisible.value = true
  // 文件夹视图关闭时，把其中正在预览的文件一并卸载，回到欢迎初始状态
  if (hadFolder && currentPath.value) await resetCurrentFile()
  try {
    await invoke('close_folder')
  } catch (_) {}
}

/// 弹出文件夹选择对话框（工具栏按钮 / 菜单 / 拖拽共用）
async function onOpenFolder() {
  try {
    const p = await invoke('plugin:dialog|open', {
      options: { directory: true, multiple: false, title: 'Select Folder' }
    })
    if (!p) return
    await openFolder(p)
  } catch (e) {
    setStatus(e.toString(), 'error')
  }
}

/// 取路径所在目录（用于「文件夹视图」默认启动时定位文件所在目录）
function dirOf(p) {
  const norm = String(p || '').replace(/\\/g, '/')
  const i = norm.lastIndexOf('/')
  if (i <= 0) return '/'
  const d = norm.slice(0, i)
  return /^[A-Za-z]:$/.test(d) ? d + '/' : d
}

/// 启动时按「默认视图」决定是否自动展开文件树
async function initStartupView(initialFile) {
  const mode = settings.viewMode
  // 启动时（含系统「打开方式」）传入的文件夹
  let launchFolder = null
  try {
    launchFolder = (await invoke('get_folder_path')) || null
  } catch (_) {}
  if (launchFolder) {
    // single 视图下忽略自动展开；folder / auto 都展开
    if (mode !== 'single') await openFolder(launchFolder)
    return
  }
  // folder 视图下打开单个文件时，自动显示其所在文件夹的文件树（不自动切 README）
  if (mode === 'folder' && initialFile) {
    const dir = dirOf(initialFile)
    if (dir) await openFolder(dir, { autoReadme: false })
  }
}

// 文件拖拽监听
async function startDropListener() {
  while (true) {
    try {
      const p = await invoke('wait_for_drop')
      if (p) {
        let kind = 'file'
        try { kind = await invoke('path_kind', { path: p }) } catch (_) {}
        if (kind === 'dir') {
          // 拖入文件夹 → 打开文件树
          await openFolder(p)
        } else {
          previewState.value = null
          await openMdFile(p)
        }
      }
    } catch (_) {
      await new Promise(r => setTimeout(r, 100))
    }
  }
}

const showAbout = ref(false)
const showSettings = ref(false)
const isDragging = ref(false)

async function onOpenFile() {
  try {
    const p = await invoke('plugin:dialog|open', {
      options: {
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        multiple: false,
        directory: false,
        title: 'Select Markdown'
      }
    })
    if (!p) return
    await openMdFile(p)
  } catch (e) {
    setStatus(e.toString(), 'error')
  }
}

// File watcher + menu polling + startup
onMounted(async () => {
  startDropListener()
  setInterval(async () => {
    try {
      if (await invoke('check_file_changed')) {
        await loadPreview()
      }
    } catch (_) {}
  }, 1500)
  setInterval(async () => {
    try {
      if (await invoke('check_show_about')) showAbout.value = true
      if (await invoke('check_show_settings')) showSettings.value = true
      if (await invoke('check_show_open_folder')) onOpenFolder()
      if (await invoke('check_show_close_folder')) closeFolder()
    } catch (_) {}
  }, 500)

  // 监听内嵌预览页转发来的内部链接点击
  window.addEventListener('message', onIframeMessage)

  // Initial file check（并以此为历史起点），随后按默认视图初始化
  let initialFile = null
  try {
    initialFile = (await invoke('get_file_path')) || null
  } catch (_) {}
  if (initialFile) {
    currentPath.value = initialFile
    history.value = [initialFile]
    historyIndex.value = 0
  }
  await initStartupView(initialFile)
  // 若启动流程（如文件夹根目录 README 自动打开）已加载预览，则不重复渲染
  invoke('get_file_name').then(n => {
    if (n && !hasFile.value) { fileName.value = n; loadPreview() }
  }).catch(() => {})
  setStatus('statusReady', 'ready')

  // 键盘快捷键：Cmd+O 打开文件（打开文件夹由菜单 Cmd/Ctrl+Shift+O 触发）；Alt+← / Alt+→ 前进后退
  window.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'o' && !e.shiftKey) {
      e.preventDefault()
      onOpenFile()
    } else if (e.altKey && e.key === 'ArrowLeft') {
      e.preventDefault()
      goBack()
    } else if (e.altKey && e.key === 'ArrowRight') {
      e.preventDefault()
      goForward()
    }
  })
})
</script>

<template>
  <div class="h-screen flex flex-col">
    <Toolbar
      :hasFile="hasFile"
      :isPdfView="isPdfView"
      :fileName="fileName"
      :can-go-back="canGoBack"
      :can-go-forward="canGoForward"
      :folder-open="isFolderView && !!folderTree"
      :file-tree-visible="fileTreeVisible"
      @open-file="onOpenFile"
      @open-folder="onOpenFolder"
      @export-pdf="onExportPdf"
      @save-pdf="onSavePdf"
      @show-html="showHtmlPreview"
      @export-doc="onExportDoc"
      @nav-back="goBack"
      @nav-forward="goForward"
      @toggle-full-width="toggleFullWidth"
      @toggle-file-tree="toggleFileTree"
    />

    <div
      class="flex flex-1 overflow-hidden relative transition-colors duration-150"
      :class="{ 'bg-[#f0f0ff]': isDragging }"
      :style="{ outline: isDragging ? '2px dashed #6366f1' : 'none', outlineOffset: isDragging ? '-2px' : '0' }"
      @dragover.prevent="isDragging = true"
      @dragleave="isDragging = false"
      @drop.prevent="isDragging = false"
    >
      <!-- 左侧文件树（文件夹视图） -->
      <FileTree
        v-if="isFolderView && folderTree && fileTreeVisible"
        :key="folderPath"
        :root-path="folderPath"
        :root-name="folderName"
        :nodes="folderTree"
        :current-path="currentPath"
        @open-file="openMdFile"
        @hide-tree="hideFileTree"
      />

      <main class="flex-1 min-w-0 overflow-hidden bg-white relative">
        <Welcome v-if="!hasFile" />

        <iframe
          v-if="hasFile && !isPdfView && previewState"
          :srcdoc="typeof previewState === 'string' ? previewState : ''"
          class="w-full h-full border-none block"
        />

        <iframe
          v-if="hasFile && isPdfView && previewState && typeof previewState !== 'string'"
          :src="'data:application/pdf;base64,' + previewState.base64"
          class="w-full h-full border-none block"
        />
      </main>
    </div>

    <StatusBar :text="statusText" :type="statusType" />

    <!-- PDF 生成中的遮罩层 -->
    <PdfOverlay v-if="isGeneratingPdf" :text="t('pdfOverlayPreparing', settings.lang)" />

    <Transition name="fade">
      <AboutDialog v-if="showAbout" @close="showAbout = false" />
    </Transition>

    <Transition name="fade">
      <SettingsDialog v-if="showSettings" @close="showSettings = false" />
    </Transition>
  </div>
</template>
