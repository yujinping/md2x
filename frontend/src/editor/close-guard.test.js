// 关闭拦截决策函数的单测。
//
// @author yujinping
import { describe, it, expect } from 'vitest'
import { confirmClose } from './close-guard.js'

describe('confirmClose', () => {
  it('无未保存修改时不弹确认直接放行', async () => {
    let asked = false
    const ok = await confirmClose(false, async () => {
      asked = true
      return true
    }, 'msg')
    expect(ok).toBe(true)
    expect(asked).toBe(false)
  })

  it('有修改且用户确认 → 放行关闭', async () => {
    const ok = await confirmClose(true, async () => true, 'msg')
    expect(ok).toBe(true)
  })

  it('有修改且用户取消 → 不关闭', async () => {
    const ok = await confirmClose(true, async () => false, 'msg')
    expect(ok).toBe(false)
  })

  it('确认框抛错（原生 API 不可用）→ 按不关闭处理', async () => {
    const ok = await confirmClose(true, async () => {
      throw new Error('dialog unavailable')
    }, 'msg')
    expect(ok).toBe(false)
  })
})
