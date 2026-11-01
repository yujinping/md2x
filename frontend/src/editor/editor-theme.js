// 编辑器的 CodeMirror 主题。
//
// 设计意图：界面骨架色（背景 / 文字 / 边框）直接引用 style.css 的 CSS 变量
// （--surface、--text、--border…），深浅色切换由应用壳层改变量完成，
// 主题对象本身不用感知档位；只有**语法配色**需要按深浅给两套
// （CodeMirror 的 HighlightStyle 不支持 CSS 变量回退到可读的档位差异）。
//
// @author yujinping
import { EditorView } from '@codemirror/view'
import { HighlightStyle } from '@codemirror/language'
import { tags as t } from '@lezer/highlight'

/// 骨架主题：深浅共用，全部走 CSS 变量与应用壳层保持一致
export const editorChromeTheme = EditorView.theme({
  '&': {
    height: '100%',
    fontSize: '13.5px',
    color: 'var(--text)',
    backgroundColor: 'var(--surface)',
  },
  '.cm-scroller': {
    fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
    lineHeight: '1.6',
  },
  '.cm-content': { caretColor: 'var(--accent)' },
  '.cm-gutters': {
    backgroundColor: 'var(--surface)',
    color: 'var(--text-dim)',
    border: 'none',
    borderRight: '1px solid var(--border)',
  },
  '.cm-activeLine': { backgroundColor: 'var(--surface-hover)' },
  '.cm-activeLineGutter': { backgroundColor: 'var(--surface-hover)' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': {
    backgroundColor: 'var(--surface-active) !important',
  },
  '.cm-cursor': { borderLeftColor: 'var(--accent)' },
  '&.cm-focused': { outline: 'none' },
})

/// 浅色语法配色
export const lightHighlight = HighlightStyle.define([
  { tag: t.heading, color: '#0550ae', fontWeight: 'bold' },
  { tag: t.strong, fontWeight: 'bold' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.link, color: '#0a3069', textDecoration: 'underline' },
  { tag: t.monospace, color: '#953800' },
  { tag: t.keyword, color: '#cf222e' },
  { tag: t.typeName, color: '#0550ae' },
  { tag: t.string, color: '#0a3069' },
  { tag: t.number, color: '#0550ae' },
  { tag: t.comment, color: '#6e7781', fontStyle: 'italic' },
  { tag: t.meta, color: '#8250df' },
  { tag: t.propertyName, color: '#953800' },
  { tag: t.variableName, color: '#24292f' },
])

/// 深色语法配色
export const darkHighlight = HighlightStyle.define([
  { tag: t.heading, color: '#79c0ff', fontWeight: 'bold' },
  { tag: t.strong, fontWeight: 'bold' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: t.link, color: '#a5d6ff', textDecoration: 'underline' },
  { tag: t.monospace, color: '#ffa657' },
  { tag: t.keyword, color: '#ff7b72' },
  { tag: t.typeName, color: '#79c0ff' },
  { tag: t.string, color: '#a5d6ff' },
  { tag: t.number, color: '#79c0ff' },
  { tag: t.comment, color: '#8b949e', fontStyle: 'italic' },
  { tag: t.meta, color: '#d2a8ff' },
  { tag: t.propertyName, color: '#ffa657' },
  { tag: t.variableName, color: '#e6edf3' },
])
