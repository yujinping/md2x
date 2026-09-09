<script setup>
import { computed, reactive, watch } from 'vue'
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
        />
      </template>
      <div
        v-else
        class="text-center text-xs mt-8 px-6 leading-relaxed"
        :style="{ color: 'var(--text-dim)' }"
      >{{ t('treeNoMarkdown', settings.lang) }}</div>
    </div>
  </aside>
</template>
