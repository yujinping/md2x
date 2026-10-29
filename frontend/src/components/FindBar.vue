<script setup>
import { ref, onMounted, nextTick } from 'vue'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const props = defineProps({
  total: { type: Number, default: 0 },
  index: { type: Number, default: 0 },
})
const emit = defineEmits(['search', 'next', 'prev', 'close'])
const settings = useSettingsStore()
const inputEl = ref(null)
const value = ref('')
// 中文输入法组词期间的中间态（拼音串）不参与检索，等 compositionend 再搜
const composing = ref(false)

onMounted(() => {
  nextTick(() => {
    if (!inputEl.value) return
    inputEl.value.focus()
    inputEl.value.select()
  })
})

function push() {
  emit('search', value.value)
}

function onInput(e) {
  value.value = e.target.value
  if (!composing.value) push()
}

function onKeydown(e) {
  // 组词过程中的回车用于提交候选，不应触发跳转
  if (e.isComposing || e.keyCode === 229) return
  if (e.key === 'Enter') {
    e.preventDefault()
    emit(e.shiftKey ? 'prev' : 'next')
  } else if (e.key === 'Escape') {
    e.preventDefault()
    emit('close')
  }
}
</script>

<template>
  <div
    class="find-bar absolute top-4 right-8 z-40 flex items-center gap-1.5 rounded-lg border shadow-lg px-2 py-1.5"
    :style="{ background: 'var(--surface)', borderColor: 'var(--border-hover)' }"
    @keydown="onKeydown"
  >
    <input
      ref="inputEl"
      type="text"
      spellcheck="false"
      :placeholder="t('findPlaceholder', settings.lang)"
      class="find-input w-[200px] px-2 py-1 text-xs rounded outline-none border"
      :style="{ background: 'var(--surface-hover)', color: 'var(--text)', borderColor: 'var(--border)' }"
      :value="value"
      @input="onInput"
      @compositionstart="composing = true"
      @compositionend="composing = false; onInput($event)"
    />

    <span
      class="text-[11px] font-mono tabular-nums w-[62px] text-right flex-shrink-0"
      :style="{ color: value && !total ? 'var(--amber)' : 'var(--text-muted)' }"
    >{{ !value ? '' : (total ? index + ' / ' + total : t('findNoResult', settings.lang)) }}</span>

    <button class="btn-icon" :title="t('findPrev', settings.lang)" :disabled="!total" @click="emit('prev')">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M8 12V4"/><path d="M5 7l3-3 3 3"/></svg>
    </button>
    <button class="btn-icon" :title="t('findNext', settings.lang)" :disabled="!total" @click="emit('next')">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M8 4v8"/><path d="M5 9l3 3 3-3"/></svg>
    </button>
    <button class="btn-icon" :title="t('btnClose', settings.lang)" @click="emit('close')">
      <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" class="w-3.5 h-3.5"><path d="M4 4l8 8"/><path d="M12 4l-8 8"/></svg>
    </button>
  </div>
</template>

<style scoped>
.find-bar {
  -webkit-app-region: no-drag;
}
.find-input:focus {
  border-color: var(--amber) !important;
}
.btn-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 26px;
  height: 26px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: all 0.15s ease;
  flex-shrink: 0;
}
.btn-icon:hover:not(:disabled) { background: var(--surface-hover); color: var(--text); }
.btn-icon:disabled { opacity: 0.35; cursor: not-allowed; }
</style>
