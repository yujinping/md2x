<script setup>
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const emit = defineEmits(['open-file'])
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
</script>

<template>
  <div class="flex flex-col items-center justify-center h-full text-center p-10" :style="{ color: 'var(--text-muted)' }">
    <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="w-12 h-12 mb-5" :style="{ color: 'var(--welcome-icon)' }">
      <rect x="6" y="6" width="20" height="36" rx="2" ry="2"/>
      <path d="M30 16l12-4v24l-12 4V16z"/><path d="M30 16l-4-2"/><path d="M42 12l-4-2"/>
      <path d="M16 20h4"/><path d="M16 26h6"/><path d="M16 32h4"/>
    </svg>

    <h2 class="text-xl font-semibold mb-2 tracking-tight" :style="{ color: 'var(--welcome-title)' }">{{ t('welcomeTitle', settings.lang) }}</h2>
    <p class="text-sm leading-relaxed max-w-[340px] mb-6" :style="{ color: 'var(--text-muted)' }">{{ t('welcomeSub', settings.lang) }}</p>

    <div class="inline-flex items-center gap-2 px-5 py-2 rounded-lg text-sm" :style="{ background: 'var(--action-hint-bg)', color: 'var(--text-dim)' }">
      <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="{ background: 'var(--kbd-bg)', border: '1px solid var(--action-hint-border)', color: 'var(--text-dim)' }">{{ shortcutMod }}</kbd>
      <span class="text-xs font-medium" :style="{ color: 'var(--text-dim)' }">+</span>
      <kbd class="inline-flex items-center justify-center min-w-[20px] h-5 px-1.5 rounded font-mono text-xs" :style="{ background: 'var(--kbd-bg)', border: '1px solid var(--action-hint-border)', color: 'var(--text-dim)' }">O</kbd>
      <span>{{ t('dropOrOpen', settings.lang) }}</span>
    </div>

    <p class="mt-8 text-xs leading-relaxed max-w-[420px]" :style="{ color: 'var(--text-dim)' }" v-html="hintHtml"></p>
  </div>
</template>
