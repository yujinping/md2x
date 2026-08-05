<script setup>
import { computed } from 'vue'

const props = defineProps({
  text: String,
  type: String,
})

const dotClass = computed(() => {
  if (props.type === 'ready') return ['bg-green-500']
  if (props.type === 'busy') return ['bg-amber', 'animate-pulse-dot']
  if (props.type === 'error') return ['bg-red-500']
  return []
})
</script>

<template>
  <footer class="flex items-center justify-between h-8 px-4 flex-shrink-0 text-[11.5px] tracking-wide transition-colors duration-300" :style="{ background: 'var(--surface)', borderTop: '1px solid var(--border)', color: 'var(--text-muted)' }">
    <div class="flex items-center gap-2">
      <span class="inline-block w-1.5 h-1.5 rounded-full transition-colors duration-300" :class="dotClass" :style="{ background: type === 'busy' ? 'var(--amber)' : type === 'ready' ? '#22c55e' : type === 'error' ? '#ef4444' : 'var(--border)' }"></span>
      <span>{{ text }}</span>
    </div>
    <div class="flex items-center gap-3 text-[11px]" :style="{ color: 'var(--text-dim)' }"></div>
  </footer>
</template>
