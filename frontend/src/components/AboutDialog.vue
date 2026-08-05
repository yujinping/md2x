<script setup>
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const emit = defineEmits(['close'])
const settings = useSettingsStore()
const version = ref('')
const copyright = ref('')

onMounted(async () => {
  try {
    const info = await invoke('get_app_info')
    version.value = 'v' + info.version
    copyright.value = info.copyright
  } catch (_) {
    version.value = 'v0.3.7'
  }
})

function onOverlayClick(e) {
  if (e.target === e.currentTarget) emit('close')
}
</script>

<template>
  <div class="fixed inset-0 z-[9998] flex items-center justify-center" :style="{ background: 'var(--overlay-bg-light)', backdropFilter: 'blur(3px)' }" @click="onOverlayClick">
    <div class="flex flex-col items-center text-center w-[380px] rounded-2xl p-10 pb-8" :style="{ background: 'var(--dialog-bg)', border: '1px solid var(--dialog-border)', boxShadow: '0 24px 80px var(--dialog-shadow)' }">
      <svg viewBox="0 0 48 48" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" class="w-12 h-12 mb-3" :style="{ color: 'var(--accent)' }">
        <rect x="6" y="6" width="20" height="36" rx="2" ry="2"/>
        <path d="M30 16l12-4v24l-12 4V16z"/><path d="M30 16l-4-2"/><path d="M42 12l-4-2"/>
      </svg>
      <div class="text-[17px] font-bold tracking-tight mb-1" :style="{ color: 'var(--text)' }">md2pdf</div>
      <div class="text-xs font-mono font-medium mb-3" :style="{ color: 'var(--accent)' }">{{ version }}</div>
      <div class="text-sm mb-4 leading-relaxed" :style="{ color: 'var(--text-muted)' }">{{ t('aboutDesc', settings.lang) }}</div>
      <div class="w-full h-px mb-4" :style="{ background: 'var(--border)' }"></div>
      <div class="flex flex-col gap-1 text-[11.5px] leading-relaxed mb-6" :style="{ color: 'var(--text-dim)' }">
        <span>{{ t('aboutTech', settings.lang) }}</span>
        <span>{{ copyright }}</span>
      </div>
      <button class="h-10 px-10 rounded-xl border-none text-sm font-semibold cursor-pointer tracking-wide transition-all duration-150 hover:scale-[1.02] active:scale-[0.98]" :style="{ background: 'var(--accent)', color: '#fff' }" @click="emit('close')">{{ t('btnClose', settings.lang) }}</button>
    </div>
  </div>
</template>
