// DBML 语法的 CodeMirror StreamLanguage 实现。
//
// 设计意图：DBML 没有官方 CodeMirror 语言包，而它的语法面很窄
// （关键字、单引号字符串、三引号 note、`//` 与 `/* */` 注释、`[...]` 属性块、类型名），
// 用 ~80 行 StreamLanguage 覆盖即可达到编辑时「看得清结构」的程度，
// 不值得为此引入一棵完整的 lezer 语法树。
//
// @author yujinping
import { StreamLanguage } from '@codemirror/language'

/// 语句级关键字（小写匹配，DBML 关键字本身也常写作大写，统一按小写判定）
const KEYWORDS = new Set(
  ('project table ref enum tablegroup note notes indexes ' +
    'pk primary key unique not null increment default ' +
    'on delete update cascade restrict set as headercolor true false').split(' ')
)

/// 常见列类型：类型名给独立颜色，扫一眼就能把「字段名 + 类型」分开
const TYPES = new Set(
  ('varchar char text int integer bigint smallint boolean bool ' +
    'datetime timestamp timestamptz date time ' +
    'decimal numeric float double real json jsonb uuid ' +
    'serial bigserial blob bytea money binary varbinary').split(' ')
)

/// StreamLanguage 的 token 规则。导出原始 spec 是为了单元测试可以
/// 直接用 StringStream 驱动它，不必起一个完整的编辑器实例。
export const dbmlSpec = {
  name: 'dbml',
  startState() {
    return {
      /// 跨行状态：null | 'comment'（/* */ 内）| 'note'（''' ''' 内）
      block: null,
    }
  },
  token(stream, state) {
    // 跨行块注释：逐段跳到下一个 `*`，是 `*/` 就收块
    if (state.block === 'comment') {
      if (stream.skipTo('*')) {
        if (stream.match('*/')) state.block = null
        else stream.next()
      } else {
        stream.skipToEnd()
      }
      return 'comment'
    }
    // 跨行三引号 note 字符串（DBML 的 Note: ''' ... '''）
    if (state.block === 'note') {
      if (stream.skipTo("'")) {
        if (stream.match("'''")) state.block = null
        else stream.next()
      } else {
        stream.skipToEnd()
      }
      return 'string'
    }

    // 行注释：//
    if (stream.match('//')) {
      stream.skipToEnd()
      return 'comment'
    }
    // 块注释起点：同行能闭合就吃满整个注释，闭合不了进入跨行状态并吃满本行
    if (stream.match('/*')) {
      if (!stream.match(/^[\s\S]*?\*\//)) {
        state.block = 'comment'
        stream.skipToEnd()
      }
      return 'comment'
    }
    // 三引号 note：同行立即闭合的不进入跨行状态
    if (stream.match("'''")) {
      if (!stream.match("'''")) state.block = 'note'
      return 'string'
    }
    // 单引号字符串（含未闭合到行尾的情况）
    if (stream.match("'")) {
      stream.skipTo("'") ? stream.next() : stream.skipToEnd()
      return 'string'
    }
    // 双引号引住的表名/列名（允许空格与保留字）
    if (stream.match('"')) {
      stream.skipTo('"') ? stream.next() : stream.skipToEnd()
      return 'propertyName'
    }
    // [...] 属性块整体一色：[pk, unique, ref: > users.id]
    if (stream.match(/\[[^\]]*\]?/)) {
      return 'meta'
    }
    // 数字（默认值、颜色 #rrggbb 之外的裸数字）
    if (stream.match(/\d+(\.\d+)?/)) {
      return 'number'
    }
    // 十六进制颜色（headercolor: #e11d48）
    if (stream.match(/#[0-9a-fA-F]{3,8}\b/)) {
      return 'number'
    }
    // 标识符：命中关键字/类型表则高亮，否则按普通变量处理
    if (stream.match(/[A-Za-z_][\w]*/)) {
      const w = stream.current().toLowerCase()
      if (KEYWORDS.has(w)) return 'keyword'
      if (TYPES.has(w)) return 'typeName'
      return 'variableName'
    }
    // 其余单字符（{ } > < , : 等）放行
    stream.next()
    return null
  },
}

export const dbmlLanguage = StreamLanguage.define(dbmlSpec)
