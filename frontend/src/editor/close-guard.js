// 窗口关闭拦截的决策函数（纯逻辑，便于单测）。
//
// 设计意图：Rust 侧拦下 CloseRequested 后发 md2x://close-requested 询问，
// 「能不能关」只有编辑器的 dirty 状态知道。这里把决策收成一个可注入
// 确认框的纯函数——App.vue 负责喂原生 ask，单测喂假 askFn。
//
// @author yujinping

/// dirty 为 false 直接放行；否则弹确认，确认 / 放弃分别返回 true / false。
/// 确认框抛错（原生 API 不可用）按「不关闭」处理：关不掉比丢稿子安全。
export async function confirmClose(dirty, askFn, msg) {
  if (!dirty) return true
  try {
    return !!(await askFn(msg))
  } catch (_) {
    return false
  }
}
