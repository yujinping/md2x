<!--
  EditorPane：源文件编辑器面板（CodeMirror 6）。

  设计意图：本组件只负责「编辑器长什么样、怎么打字」，不含任何文件读写——
  读写、冲突检测、dirty 判定全部由 App.vue 收口（见 P1 的 read_source/write_source 命令）。
  组件对外的契约只有三个：
    props.content   外部持有的文本（切换文件 / 外部重载时整体替换，光标尽力保留）
    props.dark      语法配色档位（骨架色走 CSS 变量，与档位无关）
    props.language  'markdown' | 'dbml'
    @change         文档变化（App 据此维护 dirty）
  深浅切换用 Compartment 局部换肤，不销毁视图、不丢 undo 历史。

  @author yujinping
-->
<script setup>
import { ref, watch, onMounted, onBeforeUnmount } from 'vue'
import { Compartment, EditorState } from '@codemirror/state'
import {
  EditorView,
  lineNumbers,
  highlightActiveLine,
  highlightActiveLineGutter,
  drawSelection,
  dropCursor,
  rectangularSelection,
  crosshairCursor,
} from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { keymap } from '@codemirror/view'
import { syntaxHighlighting, indentOnInput, bracketMatching } from '@codemirror/language'
import { markdown } from '@codemirror/lang-markdown'
import { dbmlLanguage } from '../editor/dbml-language.js'
import {
  editorChromeTheme,
  lightHighlight,
  darkHighlight,
} from '../editor/editor-theme.js'

const props = defineProps({
  /// 编辑器当前应显示的文本（App.vue 持有的唯一事实源）
  content: { type: String, default: '' },
  /// 语法配色深浅档
  dark: { type: Boolean, default: false },
  /// 高亮语言：'markdown' | 'dbml'
  language: { type: String, default: 'markdown' },
})

const emit = defineEmits(['change', 'editor-ready'])

const container = ref(null)
/// CodeMirror 视图实例（非响应式，挂载后创建、卸载前销毁）
let view = null

/// 主题 compartment：深浅切换只重配这一格，undo 历史与光标不动
const themeComp = new Compartment()
/// 语言 compartment：md ↔ dbml 切文件时切换
const langComp = new Compartment()

function themeExts(dark) {
  return [
    editorChromeTheme,
    syntaxHighlighting(dark ? darkHighlight : lightHighlight),
  ]
}

function langExt(language) {
  return language === 'dbml' ? dbmlLanguage : markdown()
}

onMounted(() => {
  view = new EditorView({
    state: EditorState.create({
      doc: props.content || '',
      extensions: [
        lineNumbers(),
        highlightActiveLineGutter(),
        highlightActiveLine(),
        history(),
        drawSelection(),
        dropCursor(),
        rectangularSelection(),
        crosshairCursor(),
        indentOnInput(),
        bracketMatching(),
        EditorView.lineWrapping,
        keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
        themeComp.of(themeExts(props.dark)),
        langComp.of(langExt(props.language)),
        EditorView.updateListener.of((u) => {
          if (u.docChanged) emit('change', u.state.doc.toString())
        }),
      ],
    }),
    parent: container.value,
  })
  emit('editor-ready')
})

onBeforeUnmount(() => {
  view?.destroy()
  view = null
})

/// 外部内容替换：仅在与当前文档不一致时 dispatch，避免每次 prop 变化都重置光标
watch(
  () => props.content,
  (v) => {
    if (!view) return
    const cur = view.state.doc.toString()
    if ((v || '') !== cur) {
      view.dispatch({
        changes: { from: 0, to: cur.length, insert: v || '' },
      })
    }
  }
)

watch(
  () => props.dark,
  (d) => {
    view?.dispatch({ effects: themeComp.reconfigure(themeExts(d)) })
  }
)

watch(
  () => props.language,
  (l) => {
    view?.dispatch({ effects: langComp.reconfigure(langExt(l)) })
  }
)

defineExpose({
  /// 聚焦编辑器（切换到编辑视图后由父组件调用）
  focus() {
    view?.focus()
  },
  /// 当前选中的文本（菜单「编辑 → 复制」用）；无选区返回空串
  getSelection() {
    if (!view) return ''
    const r = view.state.selection.main
    return view.state.sliceDoc(r.from, r.to)
  },
  /// 全选文档（复制前的通用前置动作）
  selectAll() {
    if (!view) return
    view.dispatch({
      selection: { anchor: 0, head: view.state.doc.length },
    })
  },
})
</script>

<template>
  <div class="w-full h-full overflow-hidden" :class="dark ? 'cm-host-dark' : 'cm-host-light'">
    <div ref="container" class="w-full h-full"></div>
  </div>
</template>

<style scoped>
.cm-host-light,
.cm-host-dark {
  background: var(--surface);
}
/* CodeMirror 6 挂载后自动撑满；高度必须显式交给内层，否则内容区塌成 0 */
.cm-host-light :deep(.cm-editor),
.cm-host-dark :deep(.cm-editor) {
  height: 100%;
}
</style>
