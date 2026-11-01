// DBML StreamLanguage 的 tokenizer 单测。
// 直接用 StringStream 驱动 dbmlSpec.token，不依赖完整编辑器环境，
// 断言「这一行应该染成什么颜色」背后的 token 类型。
//
// @author yujinping
import { describe, it, expect } from 'vitest'
import { StringStream } from '@codemirror/language'
import { dbmlSpec } from './dbml-language.js'

/// 把一行文本喂给 tokenizer，返回 token 类型序列（只保留非空 token）
function tokenizeLine(text, state = dbmlSpec.startState()) {
  const stream = new StringStream(text, 2)
  const out = []
  while (!stream.eol()) {
    stream.start = stream.pos // 真实 StreamLanguage 每次 token 前会重置起点
    const style = dbmlSpec.token(stream, state) || 'plain'
    out.push([stream.current(), style])
    if (stream.pos === stream.start) stream.next() // 防御：token 必须前进
  }
  return out
}

function tokensOf(text) {
  return tokenizeLine(text).map(([cur, style]) => `${cur}:${style}`)
}

describe('dbml tokenizer', () => {
  it('行注释 // 整行成 comment', () => {
    expect(tokensOf('// 这是注释')).toEqual(['// 这是注释:comment'])
  })

  it('块注释单行闭合不进入跨行状态', () => {
    const state = dbmlSpec.startState()
    expect(tokensOf('/* inline */ Table users', state)).toEqual([
      '/* inline */:comment',
      ' :plain',
      'Table:keyword',
      ' :plain',
      'users:variableName',
    ])
    expect(state.block).toBe(null)
  })

  it('块注释跨行持续到闭合', () => {
    const state = dbmlSpec.startState()
    expect(tokenizeLine('/* 开始', state)[0]).toEqual(['/* 开始', 'comment'])
    expect(state.block).toBe('comment')
    const mid = tokenizeLine('中间行 */ Table posts', state)
    expect(mid[0][1]).toBe('comment')
    expect(state.block).toBe(null)
    // 闭合之后的部分正常 token 化
    expect(mid).toContainEqual(['Table', 'keyword'])
  })

  it('三引号 note 字符串跨行', () => {
    const state = dbmlSpec.startState()
    tokenizeLine("Note: ''' 用户表", state)
    expect(state.block).toBe('note')
    const close = tokenizeLine("多行说明 '''", state)
    expect(close[0][1]).toBe('string')
    expect(state.block).toBe(null)
  })

  it('属性块 [...] 整体为 meta', () => {
    const out = tokensOf('id uuid [pk, unique, ref: > users.id]')
    expect(out).toContainEqual('id:variableName')
    expect(out).toContainEqual('uuid:typeName')
    expect(out).toContainEqual('[pk, unique, ref: > users.id]:meta')
  })

  it('关键字与类型名分色', () => {
    const out = tokensOf('Table users {')
    expect(out).toContainEqual('Table:keyword')
    expect(out).toContainEqual('users:variableName')
    expect(tokensOf('created_at timestamp')).toContainEqual('timestamp:typeName')
    expect(tokensOf('Ref: fk_posts_users')).toContainEqual('Ref:keyword')
  })

  it('单引号字符串与十六进制颜色', () => {
    expect(tokensOf("name varchar '无名'")).toContainEqual("'无名':string")
    expect(tokensOf('headercolor: #e11d48')).toContainEqual('#e11d48:number')
    expect(tokensOf('default 42')).toContainEqual('42:number')
  })

  it('双引号表名按 propertyName 染色', () => {
    expect(tokensOf('"order items"')).toEqual(['"order items":propertyName'])
  })

  it('双引号未闭合到行尾也不吞掉下一行状态', () => {
    const state = dbmlSpec.startState()
    tokenizeLine('"未闭合', state)
    expect(state.block).toBe(null)
  })
})
