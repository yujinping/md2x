<script setup>
import { computed, onBeforeUnmount, onMounted, reactive, watch } from 'vue'
import FileTreeNode from './FileTreeNode.vue'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const props = defineProps({
  rootPath: { type: String, default: '' },
  rootName: { type: String, default: '' },
  nodes: { type: Array, default: () => [] },
  currentPath: { type: String, default: '' },
})
const emit = defineEmits(['open-file', 'hide-tree'])

const settings = useSettingsStore()

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

// ==================== 右键菜单：复制相对路径 ====================
// 单一菜单实例：所有节点右键仅冒泡位置信息，由本组件统一渲染，避免递归组件内多处挂载
const ctxMenu = reactive({ visible: false, x: 0, y: 0, path: '', name: '' })

function onNodeContextMenu(info) {
  // 预留菜单尺寸做边缘回退，避免贴边溢出窗口
  ctxMenu.visible = true
  ctxMenu.x = Math.min(info.x, window.innerWidth - 170)
  ctxMenu.y = Math.min(info.y, window.innerHeight - 60)
  ctxMenu.path = info.path
  ctxMenu.name = info.path.replace(/\\/g, '/').split('/').pop() || info.path
}

function closeCtxMenu() {
  ctxMenu.visible = false
}

// 相对路径：剥离根目录前缀后，补上被打开文件夹自身名称作为首段
// （后端 node.path 已统一为 / 分隔的绝对路径）。
// 例：打开 docs 文件夹 → docs/guide/setup.md；无法剥离时（异常情况）退回节点名
const ctxRelPath = computed(() => {
  const root = (props.rootPath || '').replace(/\\/g, '/').replace(/\/+$/, '')
  const p = (ctxMenu.path || '').replace(/\\/g, '/')
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

// 菜单打开期间监听全局点击/滚动/Escape，任意一处即关闭
function handleGlobalClose() {
  if (ctxMenu.visible) closeCtxMenu()
}
function handleKeydown(e) {
  if (e.key === 'Escape') closeCtxMenu()
}
onMounted(() => {
  window.addEventListener('click', handleGlobalClose)
  window.addEventListener('blur', handleGlobalClose)
  window.addEventListener('keydown', handleKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('click', handleGlobalClose)
  window.removeEventListener('blur', handleGlobalClose)
  window.removeEventListener('keydown', handleKeydown)
})

const activeNorm = computed(() => (props.currentPath || '').replace(/\\/g, '/'))

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

    <!-- 文件树滚动区 -->
    <div class="flex-1 overflow-y-auto py-1.5">
      <template v-if="nodes.length">
        <FileTreeNode
          v-for="node in nodes"
          :key="node.path"
          :node="node"
          :depth="0"
          :expanded="expanded"
          :active-path="activeNorm"
          @open="openFile"
          @toggle="toggle"
          @contextmenu="onNodeContextMenu"
        />
      </template>
      <div
        v-else
        class="text-center text-xs mt-8 px-6 leading-relaxed"
        :style="{ color: 'var(--text-dim)' }"
      >{{ t('treeNoMarkdown', settings.lang) }}</div>
    </div>

    <!-- 右键菜单：复制相对路径 -->
    <Teleport to="body">
      <div
        v-if="ctxMenu.visible"
        class="fixed z-[1000] min-w-[160px] rounded-md py-1 shadow-lg"
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
      </div>
    </Teleport>
  </aside>
</template>
