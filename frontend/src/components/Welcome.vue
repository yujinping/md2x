<script setup>
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const settings = useSettingsStore()
const shortcutMod = ref('⌘')
const hintHtml = ref('')

onMounted(async () => {
  let isMac = false
  try {
    isMac = await invoke('get_platform') === 'macos'
  } catch (_) {
    isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0
  }
  shortcutMod.value = isMac ? '⌘' : 'Ctrl'
  hintHtml.value = isMac ? t('hintMac', settings.lang) : t('hintWin', settings.lang)
})

const kbdStyle = {
  background: 'var(--kbd-bg)',
  border: '1px solid var(--action-hint-border)',
  color: 'var(--text-dim)',
}
</script>

<template>
  <div class="flex flex-col items-center justify-center h-full text-center p-10" :style="{ color: 'var(--text-muted)' }">
    <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="w-12 h-12 mb-5" :style="{ color: 'var(--welcome-icon)' }">
      <rect x="6" y="6" width="20" height="36" rx="2" ry="2"/>
      <path d="M30 16l12-4v24l-12 4V16z"/><path d="M30 16l-4-2"/><path d="M42 12l-4-2"/>
      <path d="M16 20h4"/><path d="M16 26h6"/><path d="M16 32h4"/>
    </svg>

    <h2 class="text-xl font-semibold mb-2 tracking-tight" :style="{ color: 'var(--welcome-title)' }">{{ t('welcomeTitle', settings.lang) }}</h2>
    <p class="text-sm leading-relaxed max-w-[340px] mb-7" :style="{ color: 'var(--text-muted)' }">{{ t('welcomeSub', settings.lang) }}</p>

    <!-- 打开功能提示：入口在顶部导航区，此处仅作提示说明 -->
    <div class="flex flex-col items-stretch gap-2.5 w-full max-w-[360px]">
      <div class="flex items-center gap-2.5 px-4 py-2.5 rounded-lg text-xs text-left" :style="{ background: 'var(--action-hint-bg)', color: 'var(--text-dim)' }">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 flex-shrink-0" style="color: var(--text-dim)">
          <path d="M2 5l6-3 6 3v7l-6 3-6-3V5z"/><path d="M2 5l6 3 6-3"/><path d="M8 8v7"/>
        </svg>
        <span class="flex-1 min-w-0" :style="{ color: 'var(--text-dim)' }">{{ t('welcomeFileHint', settings.lang) }}</span>
        <span class="flex items-center gap-1 flex-shrink-0">
          <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="kbdStyle">{{ shortcutMod }}</kbd>
          <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="kbdStyle">O</kbd>
        </span>
      </div>
      <div class="flex items-center gap-2.5 px-4 py-2.5 rounded-lg text-xs text-left" :style="{ background: 'var(--action-hint-bg)', color: 'var(--text-dim)' }">
        <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4 flex-shrink-0" style="color: var(--text-dim)">
          <path d="M1.5 5a1 1 0 0 1 1-1h3.2l1.6 2h5.2a1 1 0 0 1 1 1v5.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V5z"/>
        </svg>
        <span class="flex-1 min-w-0" :style="{ color: 'var(--text-dim)' }">{{ t('welcomeFolderHint', settings.lang) }}</span>
        <span class="flex items-center gap-1 flex-shrink-0">
          <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="kbdStyle">⇧</kbd>
          <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="kbdStyle">{{ shortcutMod }}</kbd>
          <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="kbdStyle">O</kbd>
        </span>
      </div>
    </div>

    <p class="mt-8 text-xs leading-relaxed max-w-[420px]" :style="{ color: 'var(--text-dim)' }" v-html="hintHtml"></p>
  </div>
</template>
