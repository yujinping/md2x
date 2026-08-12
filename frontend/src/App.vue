<script setup>
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore } from './stores/settings.js'
import { t } from './i18n/index.js'
import Toolbar from './components/Toolbar.vue'
import Welcome from './components/Welcome.vue'
import StatusBar from './components/StatusBar.vue'
import AboutDialog from './components/AboutDialog.vue'
import SettingsDialog from './components/SettingsDialog.vue'
import PdfOverlay from './components/PdfOverlay.vue'

const settings = useSettingsStore()
const previewState = ref(null)
const hasFile = ref(false)
const fileName = ref('')
const statusText = ref('')
const statusType = ref('')
const isPdfView = ref(false)
const isGeneratingPdf = ref(false)
// 当前打开文件的完整路径，用于把相对链接解析为绝对路径
const currentPath = ref('')

function setStatus(msg, type) {
  statusText.value = t(msg, settings.lang)
  statusType.value = type || ''
}

// 导航历史栈（绝对路径），供工具栏前进/后退使用
const history = ref([])
const historyIndex = ref(-1)
const canGoBack = computed(() => historyIndex.value > 0)
const canGoForward = computed(
  () => historyIndex.value >= 0 && historyIndex.value < history.value.length - 1,
)

/// 把一次导航压入历史栈：与当前相同则不重复；处于历史中间则截断前进分支
function pushHistory(path) {
  if (historyIndex.value >= 0 && history.value[historyIndex.value] === path) return
  if (historyIndex.value < history.value.length - 1) {
    history.value = history.value.slice(0, historyIndex.value + 1)
  }
  history.value.push(path)
  historyIndex.value = history.value.length - 1
}

/// 打开文件并渲染预览（不改动历史栈），前进/后退也走这里
async function openFileAndRender(path) {
  currentPath.value = path
  previewState.value = null
  setStatus('statusGenerating', 'busy')
  try {
    await invoke('set_file', { path })
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  await loadPreview()
}

/// 打开文件并记入历史（链接点击 / 打开文件 / 拖拽均走此入口）
async function openMdFile(path) {
  pushHistory(path)
  await openFileAndRender(path)
}

/// 后退：回到历史中的上一份文件
async function goBack() {
  if (!canGoBack.value) return
  historyIndex.value -= 1
  await openFileAndRender(history.value[historyIndex.value])
}

/// 前进：回到历史中的下一份文件
async function goForward() {
  if (!canGoForward.value) return
  historyIndex.value += 1
  await openFileAndRender(history.value[historyIndex.value])
}

async function loadPreview() {
  let tmp
  try {
    tmp = await invoke('get_html', { fullWidth: settings.fullWidth })
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  isPdfView.value = false
  hasFile.value = true

  // Read HTML into srcdoc
  let html = ''
  try {
    let off = 0, done = false
    while (!done) {
      const [chunk, last] = await invoke('read_file_chunk', { path: tmp, offset: off })
      html += chunk
      off += 524288
      done = last
    }
  } catch (e) {
    setStatus(e.toString(), 'error')
    return
  }
  previewState.value = injectLinkHandler(html)

  try {
    const n = await invoke('get_file_name')
    fileName.value = n || ''
  } catch (_) {}

  setStatus('statusPreviewReady', 'ready')
}

/// 向内嵌预览 HTML 注入点击拦截脚本：
/// 点击指向 .md 的内部链接时，阻止 iframe 自行跳转（否则会嵌套空页面），
/// 改为通知父窗口在应用内打开对应文件。
function injectLinkHandler(html) {
  const open = '<' + 'script>'
  const close = '<' + '/script>'
  const code = `
(function () {
  document.addEventListener('click', function (e) {
    var a = e.target && e.target.closest ? e.target.closest('a') : null;
    if (!a) return;
    var href = a.getAttribute && a.getAttribute('href');
    if (!href) return;
    // 仅拦截指向 .md 的内部链接（相对路径或 file://），锚点与外链放行
    if (/\\.md([?#]|$)/i.test(href)) {
      e.preventDefault();
      if (window.parent && window.parent !== window) {
        window.parent.postMessage({ type: 'md2x-link', href: href }, '*');
      }
    }
  }, true);
})();
`
  const script = open + code + close
  if (html.indexOf('</body>') !== -1) {
    return html.replace('</body>', script + '</body>')
  }
  return html + script
}

/// 处理 iframe 转发来的内部链接点击：解析为绝对路径并在应用内打开
async function resolveAndOpen(href) {
  // 以当前打开文件所在目录为基准解析相对链接；
  // 若前端记录的当前路径为空（极端时序），回退到 Rust 端权威路径
  let basePath = currentPath.value
  if (!basePath) {
    try { basePath = (await invoke('get_file_path')) || '' } catch (_) {}
  }
  if (!basePath) return

  let target
  if (href.startsWith('file://')) {
    target = decodeURIComponent(href.replace(/^file:\/\//, ''))
    if (!target.startsWith('/') && !target.startsWith('//')) target = '/' + target
  } else {
    // 相对路径：以当前打开文件所在目录为基准解析（支持 ./ ../ 与 %20 等编码）
    const dir = basePath.replace(/[\\/][^\\/]*$/, '')
    const base = 'file://' + (dir.startsWith('/') ? '' : '/') + dir + '/'
    try {
      // new URL 会把中文等非 ASCII 字符做百分号编码，pathname 不是真实文件路径，
      // 必须 decodeURIComponent 还原成文件系统真实路径，否则 set_file 找不到文件
      target = decodeURIComponent(new URL(href, base).pathname)
    } catch (err) {
      return
    }
  }
  if (!target || !/\.md$/i.test(target)) return
  // 在应用内以 HTML 方式打开该 .md（openMdFile 会重新渲染预览）
  openMdFile(target)
}

function onIframeMessage(e) {
  const data = e.data
  if (!data || data.type !== 'md2x-link') return
  resolveAndOpen(data.href)
}

async function onExportPdf() {
  if (previewState.value && isPdfView.value) {
    showPdfViewer(previewState.value)
    return
  }

  // 先显示遮罩层，将 PDF 生成推到下一个 macrotask
  // 确保浏览器有时间渲染遮罩层
  isGeneratingPdf.value = true
  await new Promise(resolve => setTimeout(resolve, 50))

  setStatus('statusGeneratingPdf', 'busy')
  try {
    const r = await invoke('preview_pdf', { fullWidth: settings.fullWidth })
    previewState.value = r
    showPdfViewer(r)
    setStatus('statusPdfReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  } finally {
    isGeneratingPdf.value = false
  }
}

function showPdfViewer(r) {
  isPdfView.value = true
  fileName.value = r.file_name || ''
}

async function onSavePdf() {
  if (!previewState.value || typeof previewState.value === 'string') {
    setStatus('statusNoPdf', 'error')
    return
  }
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: 'PDF 文档', extensions: ['pdf'] }],
        defaultPath: previewState.value.file_name,
        title: 'Save PDF'
      }
    })
    if (!d) return
    setStatus('statusSaving', 'busy')
    await invoke('save_pdf_as', { src: previewState.value.temp_path, dst: d })
    setStatus('statusPdfSaved', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

async function onExportDoc(format) {
  const ext = format
  const nameMap = { html: 'HTML 文档', pdf: 'PDF 文档', docx: 'Word 文档' }
  const base = (fileName.value || 'document').replace(/\.md$/i, '')
  try {
    const d = await invoke('plugin:dialog|save', {
      options: {
        filters: [{ name: nameMap[format], extensions: [ext] }],
        defaultPath: base + '.' + ext,
        title: 'Export ' + ext.toUpperCase()
      }
    })
    if (!d) return
    setStatus('statusExporting', 'busy')
    if (format === 'docx') {
      await invoke('export_docx', { dst: d })
    } else {
      await invoke('export_' + format, { dst: d, fullWidth: settings.fullWidth })
    }
    setStatus('statusExportReady', 'ready')
  } catch (err) {
    setStatus(err.toString(), 'error')
  }
}

function toggleFullWidth() {
  settings.setFullWidth(!settings.fullWidth)
  // 重新渲染预览以应用新的宽度模式（PDF 视图则切回 HTML 预览）
  if (isPdfView.value) isPdfView.value = false
  loadPreview()
}

function showHtmlPreview() {
  isPdfView.value = false
  // 从 PDF 视图返回时重新加载 HTML 预览
  loadPreview()
}

// 文件拖拽监听
async function startDropListener() {
  while (true) {
    try {
      const p = await invoke('wait_for_drop')
      if (p) {
        previewState.value = null
        await openMdFile(p)
      }
    } catch (_) {
      await new Promise(r => setTimeout(r, 100))
    }
  }
}

// File watcher + menu polling
onMounted(() => {
  startDropListener()
  setInterval(async () => {
    try {
      if (await invoke('check_file_changed')) {
        await loadPreview()
      }
    } catch (_) {}
  }, 1500)
  setInterval(async () => {
    try {
      if (await invoke('check_show_about')) showAbout.value = true
      if (await invoke('check_show_settings')) showSettings.value = true
    } catch (_) {}
  }, 500)
  // 监听内嵌预览页转发来的内部链接点击
  window.addEventListener('message', onIframeMessage)
  // Initial file check（并以此为历史起点）
  invoke('get_file_path').then(p => {
    if (p) { currentPath.value = p; history.value = [p]; historyIndex.value = 0 }
  }).catch(() => {})
  invoke('get_file_name').then(n => {
    if (n) { fileName.value = n; loadPreview() }
  }).catch(() => {})
  setStatus('statusReady', 'ready')

  // 键盘快捷键：Cmd+O / Ctrl+O 打开文件；Alt+← / Alt+→ 前进后退
  window.addEventListener('keydown', (e) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'o') {
      e.preventDefault()
      onOpenFile()
    } else if (e.altKey && e.key === 'ArrowLeft') {
      e.preventDefault()
      goBack()
    } else if (e.altKey && e.key === 'ArrowRight') {
      e.preventDefault()
      goForward()
    }
  })
})

const showAbout = ref(false)
const showSettings = ref(false)
const isDragging = ref(false)

async function onOpenFile() {
  try {
    const p = await invoke('plugin:dialog|open', {
      options: {
        filters: [{ name: 'Markdown', extensions: ['md'] }],
        multiple: false,
        directory: false,
        title: 'Select Markdown'
      }
    })
    if (!p) return
    await openMdFile(p)
  } catch (e) {
    setStatus(e.toString(), 'error')
  }
}
</script>

<template>
  <div class="h-screen flex flex-col">
    <Toolbar
      :hasFile="hasFile"
      :isPdfView="isPdfView"
      :fileName="fileName"
      :can-go-back="canGoBack"
      :can-go-forward="canGoForward"
      @open-file="onOpenFile"
      @export-pdf="onExportPdf"
      @save-pdf="onSavePdf"
      @show-html="showHtmlPreview"
      @export-doc="onExportDoc"
      @nav-back="goBack"
      @nav-forward="goForward"
      @toggle-full-width="toggleFullWidth"
    />

    <main
      class="flex-1 overflow-hidden bg-white relative transition-colors duration-150"
      :class="{ 'bg-[#f0f0ff]': isDragging }"
      :style="{ outline: isDragging ? '2px dashed #6366f1' : 'none', outlineOffset: isDragging ? '-2px' : '0' }"
      @dragover.prevent="isDragging = true"
      @dragleave="isDragging = false"
      @drop.prevent="isDragging = false"
    >
      <Welcome
        v-if="!hasFile"
        @open-file="onOpenFile"
      />

      <iframe
        v-if="hasFile && !isPdfView && previewState"
        :srcdoc="typeof previewState === 'string' ? previewState : ''"
        class="w-full h-full border-none block"
      />

      <iframe
        v-if="hasFile && isPdfView && previewState && typeof previewState !== 'string'"
        :src="'data:application/pdf;base64,' + previewState.base64"
        class="w-full h-full border-none block"
      />
    </main>

    <StatusBar :text="statusText" :type="statusType" />

    <!-- PDF 生成中的遮罩层 -->
    <PdfOverlay v-if="isGeneratingPdf" :text="t('pdfOverlayPreparing', settings.lang)" />

    <Transition name="fade">
      <AboutDialog v-if="showAbout" @close="showAbout = false" />
    </Transition>

    <Transition name="fade">
      <SettingsDialog v-if="showSettings" @close="showSettings = false" />
    </Transition>
  </div>
</template>
