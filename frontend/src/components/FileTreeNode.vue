<script setup>
import { computed } from 'vue'

const props = defineProps({
  node: { type: Object, required: true },
  depth: { type: Number, default: 0 },
  expanded: { type: Set, required: true },
  activePath: { type: String, default: '' },
})
const emit = defineEmits(['open', 'toggle'])

const isDir = computed(() => props.node.isDir)
const isOpen = computed(() => isDir.value && props.expanded.has(props.node.path))
const isActive = computed(() => !isDir.value && props.node.path === props.activePath)

function onClick() {
  if (isDir.value) emit('toggle', props.node.path)
  else emit('open', props.node.path)
}
</script>

<template>
  <div>
    <!-- 行主体：文件夹点击展开/收起，文件点击打开 -->
    <div
      class="flex items-center gap-1.5 pr-2 py-[3px] cursor-pointer select-none transition-colors duration-100"
      :style="{
        paddingLeft: depth * 14 + 10 + 'px',
        background: isActive ? 'rgba(245,158,11,0.14)' : 'transparent',
      }"
      @click="onClick"
      @dblclick.stop
    >
      <!-- 展开箭头（文件夹） -->
      <svg
        v-if="isDir"
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="w-3 h-3 flex-shrink-0 transition-transform duration-100"
        :style="{
          transform: isOpen ? 'rotate(90deg)' : 'rotate(0deg)',
          color: isOpen ? 'var(--text)' : 'var(--text-dim)',
        }"
      ><path d="M6 4l4 4-4 4" /></svg>
      <span v-else class="w-3 h-3 flex-shrink-0" />

      <!-- 图标：文件夹 / 文件 -->
      <svg
        viewBox="0 0 16 16"
        fill="none"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="w-[15px] h-[15px] flex-shrink-0"
        :style="{ color: isDir ? 'var(--amber)' : isActive ? 'var(--amber)' : 'var(--text-muted)' }"
      >
        <template v-if="isDir">
          <path d="M1.5 4.5a1 1 0 0 1 1-1h3.2l1.6 2h5.2a1 1 0 0 1 1 1v5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1v-7z" />
        </template>
        <template v-else>
          <path d="M4 1.5h5l3 3v10H4z" />
          <path d="M9 1.5v3h3" />
          <path d="M6.5 9h3" />
          <path d="M6.5 11.5h3" />
        </template>
      </svg>

      <!-- 名称 -->
      <span
        class="truncate text-[12.5px] leading-5"
        :style="{
          color: isActive ? 'var(--amber)' : isDir ? 'var(--text)' : 'var(--text-muted)',
          fontWeight: isActive ? 600 : 400,
        }"
        :title="node.path"
      >{{ node.name }}</span>
    </div>

    <!-- 子节点递归 -->
    <template v-if="isDir && isOpen">
      <FileTreeNode
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
        :expanded="expanded"
        :active-path="activePath"
        @open="p => emit('open', p)"
        @toggle="p => emit('toggle', p)"
      />
    </template>
  </div>
</template>
