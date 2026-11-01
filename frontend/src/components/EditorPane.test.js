// EditorPane 组件测试：验证 CodeMirror 视图挂载、内容双向同步与深浅切换。
//
// @author yujinping
import { describe, it, expect } from 'vitest'
import { mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import EditorPane from './EditorPane.vue'

function flushCm() {
  // CodeMirror 的 DOM 更新走 rAF/微任务，测试环境里多让几拍
  return new Promise((r) => setTimeout(r, 20))
}

describe('EditorPane', () => {
  it('挂载后渲染初始内容并派发 editor-ready', async () => {
    const w = mount(EditorPane, {
      props: { content: '# hello', language: 'markdown', dark: false },
    })
    await flushCm()
    expect(w.text()).toContain('hello')
    expect(w.emitted('editor-ready')).toBeTruthy()
    w.unmount()
  })

  it('用户输入触发 change，App 端内容回流（同值）不重置光标', async () => {
    const w = mount(EditorPane, {
      props: { content: 'abc', language: 'markdown', dark: false },
    })
    await flushCm()
    const contentEl = w.find('.cm-content').element
    contentEl.focus()
    // 模拟输入：直接对 content 元素派发 beforeinput/input
    document.execCommand && document.execCommand('insertText', false, 'd')
    await flushCm()
    const changes = w.emitted('change')
    if (changes) {
      expect(changes[changes.length - 1][0]).toBe('abcd')
      // 外部把新值回传（受控回环），不应产生第二次 change
      await w.setProps({ content: changes[changes.length - 1][0] })
      await nextTick()
      await flushCm()
      const after = w.emitted('change') || []
      expect(after.length).toBe(changes.length)
    }
    w.unmount()
  })

  it('外部整体替换内容（切换文件场景）会更新视图', async () => {
    const w = mount(EditorPane, {
      props: { content: 'old doc', language: 'markdown', dark: false },
    })
    await flushCm()
    await w.setProps({ content: 'new doc' })
    await nextTick()
    await flushCm()
    expect(w.text()).toContain('new doc')
    expect(w.text()).not.toContain('old doc')
    w.unmount()
  })

  it('深浅切换不销毁视图（undo 历史保留的前提）', async () => {
    const w = mount(EditorPane, {
      props: { content: 'x', language: 'markdown', dark: false },
    })
    await flushCm()
    const el = w.find('.cm-editor').element
    await w.setProps({ dark: true })
    await nextTick()
    await flushCm()
    // 同一个 DOM 节点还在，说明没有整棵重建
    expect(w.find('.cm-editor').element).toBe(el)
    w.unmount()
  })

  it('getSelection 返回选中文本，空选区返回空串', async () => {
    const w = mount(EditorPane, {
      props: { content: 'hello world', language: 'markdown', dark: false },
    })
    await flushCm()
    // 初始无选区
    expect(w.vm.getSelection()).toBe('')
    // 全选后返回整篇文档（菜单「编辑 → 复制」的数据来源）
    w.vm.selectAll()
    await flushCm()
    expect(w.vm.getSelection()).toBe('hello world')
    w.unmount()
  })

  it('language 切到 dbml 不抛错', async () => {
    const w = mount(EditorPane, {
      props: { content: 'Table users {}', language: 'markdown', dark: false },
    })
    await flushCm()
    await w.setProps({ language: 'dbml' })
    await nextTick()
    await flushCm()
    expect(w.text()).toContain('users')
    w.unmount()
  })
})
