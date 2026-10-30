<script setup>
import { computed } from 'vue'

const props = defineProps({
  node: { type: Object, required: true },
  depth: { type: Number, default: 0 },
  expanded: { type: Set, required: true },
  activePath: { type: String, default: '' },
  /// 正在被拖动的节点相对路径（空串表示没有拖动进行中）
  draggingRel: { type: String, default: '' },
  /// 当前插入位置提示：{ rel, pos: 'before' | 'after' } | null
  dropTarget: { type: Object, default: null },
  /// 绝对路径 → 相对根路径的换算函数（由 FileTree 注入，避免每个节点各写一遍）
  relOf: { type: Function, required: true },
})
const emit = defineEmits(['open', 'toggle', 'contextmenu', 'drag-start'])

const isDir = computed(() => props.node.isDir)
const isOpen = computed(() => isDir.value && props.expanded.has(props.node.path))
const isActive = computed(() => !isDir.value && props.node.path === props.activePath)
const isExcluded = computed(() => !!props.node.excluded)

const rel = computed(() => props.relOf(props.node.path))
/// 自己正被拖动：整行淡出，表示「正在搬这一行」
const isDragging = computed(() => !!props.draggingRel && props.draggingRel === rel.value)
/// 落点提示：只在「插到自己前面/后面」时画线，插到自己身上不画
const dropPos = computed(() => {
  const t = props.dropTarget
  if (!t || t.rel !== rel.value) return ''
  return t.pos
})

function onClick() {
  if (isDir.value) emit('toggle', props.node.path)
  else emit('open', props.node.path)
}

// 右键菜单：把节点信息与鼠标位置上抛给 FileTree 统一渲染
// 拖动不使用 HTML5 drag（见 FileTree 里的说明），由父级统一用鼠标事件处理，
// 本组件只负责渲染 data-* 供父级命中，以及落点线/淡出这两个视觉状态。
function onMouseDown(e) {
  emit('drag-start', { path: props.node.path, isDir: isDir.value }, e)
}

function onContextMenu(e) {
  emit('contextmenu', {
    path: props.node.path,
    isDir: isDir.value,
    excluded: isExcluded.value,
    x: e.clientX,
    y: e.clientY,
  })
}
</script>

<template>
  <div>
    <!-- 行主体：文件夹点击展开/收起，文件点击打开 -->
    <div
      class="relative flex items-center gap-1.5 pr-2 py-[3px] cursor-pointer select-none transition-colors duration-100"
      :style="{
        paddingLeft: depth * 14 + 10 + 'px',
        background: isActive ? 'rgba(245,158,11,0.14)' : 'transparent',
        opacity: isDragging ? 0.4 : isExcluded ? 0.45 : 1,
      }"
      :data-rel="rel"
      :data-path="node.path"
      :data-isdir="isDir ? '1' : '0'"
      @click="onClick"
      @contextmenu.prevent.stop="onContextMenu"
      @dblclick.stop
      @mousedown="onMouseDown"
    >
      <!-- 插入位置指示线：贴在本行的上沿或下沿 -->
      <div
        v-if="dropPos"
        class="absolute left-0 right-0 pointer-events-none"
        :style="{
          [dropPos === 'before' ? 'top' : 'bottom']: '-1px',
          height: '2px',
          background: 'var(--amber)',
          borderRadius: '1px',
        }"
      />

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

      <!-- 名称：被排除的加删除线，一眼看出「这篇不进产物」 -->
      <span
        class="truncate text-[12.5px] leading-5"
        :style="{
          color: isActive ? 'var(--amber)' : isDir ? 'var(--text)' : 'var(--text-muted)',
          fontWeight: isActive ? 600 : 400,
          textDecoration: isExcluded ? 'line-through' : 'none',
        }"
        :title="isExcluded ? node.path + '（已排除）' : node.path"
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
        :dragging-rel="draggingRel"
        :drop-target="dropTarget"
        :rel-of="relOf"
        @open="p => emit('open', p)"
        @toggle="p => emit('toggle', p)"
        @contextmenu="p => emit('contextmenu', p)"
        @drag-start="(p, e) => emit('drag-start', p, e)"
      />
    </template>
  </div>
</template>