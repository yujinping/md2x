<script setup>
defineProps({
  text: String,
})
</script>

<template>
  <div class="fixed inset-0 z-[9999] flex items-center justify-center" :style="{ background: 'var(--overlay-bg)', backdropFilter: 'blur(4px)' }">
    <div class="flex flex-col items-center gap-5 rounded-2xl px-12 py-10" :style="{ background: 'var(--dialog-bg)', border: '1px solid var(--dialog-border)', boxShadow: '0 20px 60px var(--dialog-shadow)' }">
      <span class="w-8 h-8 rounded-full border-3" :style="{ borderColor: 'var(--border)', borderTopColor: 'var(--accent)' }" style="animation: overlay-spin 0.7s linear infinite"></span>
      <span class="text-sm tracking-wide" :style="{ color: 'var(--text-muted)' }">{{ text }}</span>
    </div>
  </div>
</template>
