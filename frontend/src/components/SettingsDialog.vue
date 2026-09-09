<script setup>
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from '../stores/settings.js'
import { t } from '../i18n/index.js'

const emit = defineEmits(['close'])
const settings = useSettingsStore()

const platform = ref('')
const defaultBusy = ref(false)
const defaultMsg = ref(null) // { ok: boolean, text: string } | null

onMounted(async () => {
  try {
    platform.value = await invoke('get_platform')
  } catch (_) {}
})

const viewModes = [
  { value: 'single', labelKey: 'viewSingle', descKey: 'viewSingleDesc' },
  { value: 'folder', labelKey: 'viewFolder', descKey: 'viewFolderDesc' },
  { value: 'auto', labelKey: 'viewAuto', descKey: 'viewAutoDesc' },
]
const activeViewMode = computed(
  () => viewModes.find(m => m.value === settings.viewMode) || viewModes[2],
)

function onOverlayClick(e) {
  if (e.target === e.currentTarget) emit('close')
}

async function setDefault() {
  if (defaultBusy.value) return
  defaultBusy.value = true
  defaultMsg.value = null
  try {
    await invoke('register_default_md_handler')
    const key =
      platform.value === 'macos'
        ? 'defaultSetOkMac'
        : platform.value === 'windows'
          ? 'defaultSetOkWin'
          : 'defaultSetOkLinux'
    defaultMsg.value = { ok: true, text: t(key, settings.lang) }
  } catch (e) {
    defaultMsg.value = { ok: false, text: t('defaultSetFail', settings.lang) + String(e) }
  } finally {
    defaultBusy.value = false
  }
}
</script>

<template>
  <div
    class="fixed inset-0 z-[9998] flex items-center justify-center"
    :style="{ background: 'var(--overlay-bg-light)', backdropFilter: 'blur(3px)' }"
    @click="onOverlayClick"
  >
    <div
      class="w-[480px] max-h-[92vh] rounded-2xl flex flex-col"
      :style="{
        background: 'var(--dialog-bg)',
        border: '1px solid var(--dialog-border)',
        boxShadow: '0 24px 80px var(--dialog-shadow)',
      }"
    >
      <!-- Header -->
      <div class="flex items-center justify-between px-7 pt-6 pb-3 flex-shrink-0">
        <span class="text-base font-bold tracking-tight" :style="{ color: 'var(--text)' }">
          {{ t('settingsTitle', settings.lang) }}
        </span>
      </div>

      <!-- Body -->
      <div class="flex flex-col px-7 pb-7 overflow-y-auto">
        <!-- Theme -->
        <div class="mb-6">
          <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsTheme', settings.lang) }}</div>
          <div class="flex flex-col gap-2.5">
            <button
              class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
              :style="{
                border: '1.5px solid ' + (settings.theme === 'dark' ? 'var(--accent)' : 'var(--border)'),
                background: settings.theme === 'dark' ? 'rgba(99,102,241,0.06)' : 'transparent'
              }"
              @click="settings.setTheme('dark')">
              <span class="flex items-center justify-center w-10 h-10 rounded-xl flex-shrink-0" :style="{ background: '#1a1d28', border: '1px solid #2a2e3e' }">
                <svg viewBox="0 0 24 24" fill="currentColor" class="w-5 h-5" style="color: #f59e0b; transform: scaleX(-1)">
                  <path fill-rule="evenodd" d="M9.528 1.718a.75.75 0 01.162.819A8.97 8.97 0 009 6a9 9 0 009 9 8.97 8.97 0 003.463-.69.75.75 0 01.981.98 10.503 10.503 0 01-9.694 6.46c-5.799 0-10.5-4.701-10.5-10.5 0-4.368 2.667-8.112 6.46-9.694a.75.75 0 01.818.162z" clip-rule="evenodd"/>
                </svg>
              </span>
              <div class="flex flex-col items-start">
                <span class="text-sm font-medium" :style="{ color: settings.theme === 'dark' ? 'var(--accent)' : 'var(--text)' }">{{ t('themeDark', settings.lang) }}</span>
                <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">深色界面，护眼舒适</span>
              </div>
            </button>
            <button
              class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
              :style="{
                border: '1.5px solid ' + (settings.theme === 'light' ? 'var(--accent)' : 'var(--border)'),
                background: settings.theme === 'light' ? 'rgba(99,102,241,0.06)' : 'transparent'
              }"
              @click="settings.setTheme('light')">
              <span class="flex items-center justify-center w-10 h-10 rounded-xl flex-shrink-0" :style="{ background: '#ffffff', border: '1px solid #d4d6da' }">
                <svg viewBox="0 0 18 18" fill="none" stroke="#1d1d1f" stroke-width="1.5" class="w-5 h-5">
                  <circle cx="9" cy="9" r="3"/>
                  <path d="M9 1v2M9 15v2M1 9h2M15 9h2M3.5 3.5l1.5 1.5M13 13l1.5 1.5M3.5 14.5l1.5-1.5M13 5l1.5-1.5"/>
                </svg>
              </span>
              <div class="flex flex-col items-start">
                <span class="text-sm font-medium" :style="{ color: settings.theme === 'light' ? 'var(--accent)' : 'var(--text)' }">{{ t('themeLight', settings.lang) }}</span>
                <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">亮色界面，清晰明亮</span>
              </div>
            </button>
          </div>
        </div>

        <!-- Divider -->
        <div class="w-full h-px mb-6" :style="{ background: 'var(--border)' }"></div>

        <!-- Language -->
        <div class="mb-6">
          <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsLang', settings.lang) }}</div>
          <div class="flex flex-col gap-2.5">
            <button
              class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
              :style="{
                border: '1.5px solid ' + (settings.lang === 'zh-CN' ? 'var(--accent)' : 'var(--border)'),
                background: settings.lang === 'zh-CN' ? 'rgba(99,102,241,0.06)' : 'transparent'
              }"
              @click="settings.setLang('zh-CN')">
              <span class="text-xl flex-shrink-0 w-10 h-10 flex items-center justify-center">🇨🇳</span>
              <div class="flex flex-col items-start">
                <span class="text-sm font-medium" :style="{ color: settings.lang === 'zh-CN' ? 'var(--accent)' : 'var(--text)' }">中文</span>
                <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">简体中文</span>
              </div>
            </button>
            <button
              class="flex items-center gap-4 w-full px-4 py-3 rounded-xl cursor-pointer transition-all duration-200"
              :style="{
                border: '1.5px solid ' + (settings.lang === 'en' ? 'var(--accent)' : 'var(--border)'),
                background: settings.lang === 'en' ? 'rgba(99,102,241,0.06)' : 'transparent'
              }"
              @click="settings.setLang('en')">
              <span class="text-xl flex-shrink-0 w-10 h-10 flex items-center justify-center">🇺🇸</span>
              <div class="flex flex-col items-start">
                <span class="text-sm font-medium" :style="{ color: settings.lang === 'en' ? 'var(--accent)' : 'var(--text)' }">English</span>
                <span class="text-[11px]" :style="{ color: 'var(--text-dim)' }">US English</span>
              </div>
            </button>
          </div>
        </div>

        <!-- Divider -->
        <div class="w-full h-px mb-6" :style="{ background: 'var(--border)' }"></div>

        <!-- Default view -->
        <div class="mb-6">
          <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsView', settings.lang) }}</div>
          <div class="grid grid-cols-3 gap-2 mb-2.5">
            <button
              v-for="m in viewModes"
              :key="m.value"
              class="px-2 py-2.5 rounded-xl text-[13px] font-medium cursor-pointer transition-all duration-200"
              :style="{
                border: '1.5px solid ' + (settings.viewMode === m.value ? 'var(--accent)' : 'var(--border)'),
                background: settings.viewMode === m.value ? 'rgba(99,102,241,0.08)' : 'transparent',
                color: settings.viewMode === m.value ? 'var(--accent)' : 'var(--text)',
              }"
              @click="settings.setViewMode(m.value)"
            >{{ t(m.labelKey, settings.lang) }}</button>
          </div>
          <p class="text-xs leading-relaxed mb-1.5" :style="{ color: 'var(--text-muted)' }">
            {{ t(activeViewMode.descKey, settings.lang) }}
          </p>
          <p class="text-[11px]" :style="{ color: 'var(--text-dim)' }">{{ t('viewNote', settings.lang) }}</p>
        </div>

        <!-- Divider -->
        <div class="w-full h-px mb-6" :style="{ background: 'var(--border)' }"></div>

        <!-- Default app -->
        <div class="mb-6">
          <div class="text-xs font-semibold uppercase tracking-wider mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('settingsDefaultApp', settings.lang) }}</div>
          <p class="text-xs leading-relaxed mb-3" :style="{ color: 'var(--text-muted)' }">{{ t('defaultAppDesc', settings.lang) }}</p>
          <button
            class="h-10 px-5 rounded-xl border-none text-sm font-semibold cursor-pointer tracking-wide transition-all duration-150 hover:scale-[1.02] active:scale-[0.98] flex items-center justify-center gap-2"
            :style="{ background: 'var(--amber)', color: '#fff', opacity: defaultBusy ? 0.6 : 1 }"
            :disabled="defaultBusy"
            @click="setDefault"
          >
            <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" class="w-4 h-4">
              <path d="M8 1.5a6.5 6.5 0 1 0 6.5 6.5" />
              <path d="M8 5.5V8l1.8 1.8" />
            </svg>
            {{ defaultBusy ? '…' : t('btnSetDefault', settings.lang) }}
          </button>
          <p v-if="defaultMsg" class="mt-2.5 text-xs leading-relaxed" :style="{ color: defaultMsg.ok ? 'var(--accent)' : '#ef4444' }">
            {{ defaultMsg.text }}
          </p>
          <p class="mt-2.5 text-[11px] leading-relaxed" :style="{ color: 'var(--text-dim)' }">{{ t('defaultAppNote', settings.lang) }}</p>
        </div>

        <!-- Close button -->
        <button
          class="h-10 rounded-xl border-none text-sm font-semibold cursor-pointer tracking-wide transition-all duration-150 hover:scale-[1.02] active:scale-[0.98] flex-shrink-0"
          :style="{ background: 'var(--accent)', color: '#fff' }"
          @click="emit('close')">{{ t('btnClose', settings.lang) }}</button>
      </div>
    </div>
  </div>
</template>
