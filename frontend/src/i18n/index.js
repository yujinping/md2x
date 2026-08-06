export const i18n = {
  'zh-CN': {
    settingsTitle: '设置', settingsTheme: '主题', themeDark: '深色', themeLight: '浅色',
    settingsLang: '语言', btnClose: '关闭',
    btnOpen: '打开文件', btnPreviewPdf: '预览 PDF', btnBack: '返回预览', btnSave: '保存 PDF',
    welcomeTitle: '准备就绪', welcomeSub: '打开一个 Markdown 文档，即可预览并导出为 PDF',
    statusReady: '就绪', statusGenerating: '正在生成预览…',
    statusPreviewReady: '预览已生成', statusGeneratingPdf: '正在生成 PDF…',
    statusPdfReady: 'PDF 已生成', statusSaving: '正在保存…', statusPdfSaved: 'PDF 已保存',
    statusNoPdf: '没有可保存的 PDF',
    aboutDesc: 'Markdown 一键预览，导出精美 PDF',
    aboutTech: '基于 Tauri v2 · Chrome 无头渲染',
    pdfOverlayPreparing: '正在准备 PDF 预览…',
    dropOrOpen: '或拖入 .md 文件',
    hintMac: '在 Finder 中将 .md 设为默认打开程序：<br>右键 .md 文件 → 显示简介 → 打开方式 → md2x → 全部更改',
    hintWin: '在资源管理器中将 .md 设为默认打开程序：<br>右键 .md 文件 → 打开方式 → 选择其他应用 → md2x → 始终使用',
    apiUnavailable: 'Tauri API 不可用',
  },
  'en': {
    settingsTitle: 'Settings', settingsTheme: 'Theme', themeDark: 'Dark', themeLight: 'Light',
    settingsLang: 'Language', btnClose: 'Close',
    btnOpen: 'Open File', btnPreviewPdf: 'Preview PDF', btnBack: '← Back', btnSave: 'Save PDF',
    welcomeTitle: 'Ready', welcomeSub: 'Open a Markdown file to preview and export as PDF',
    statusReady: 'Ready', statusGenerating: 'Generating preview…',
    statusPreviewReady: 'Preview ready', statusGeneratingPdf: 'Generating PDF…',
    statusPdfReady: 'PDF ready', statusSaving: 'Saving…', statusPdfSaved: 'PDF saved',
    statusNoPdf: 'No PDF to save',
    aboutDesc: 'Instant Markdown preview with beautiful PDF export',
    aboutTech: 'Powered by Tauri v2 · Chrome headless',
    pdfOverlayPreparing: 'Preparing PDF preview…',
    dropOrOpen: 'or drop a .md file',
    hintMac: 'Set .md as default in Finder:<br>Right-click .md → Get Info → Open with → md2x → Change All',
    hintWin: 'Set .md as default in File Explorer:<br>Right-click .md → Open with → Choose another app → md2x → Always use',
    apiUnavailable: 'Tauri API unavailable',
  }
}

const FALLBACK = 'zh-CN'

export function t(key, lang) {
  const locale = lang || localStorage.getItem('mpe-lang') || FALLBACK
  return (i18n[locale] && i18n[locale][key]) || (i18n[FALLBACK][key]) || key
}

export function reverseLookup(text, targetLang) {
  for (const key in i18n[FALLBACK]) {
    if (i18n[FALLBACK][key] === text && i18n[targetLang]?.[key]) {
      return i18n[targetLang][key]
    }
  }
  for (const key in i18n['en']) {
    if (i18n['en'][key] === text && i18n[targetLang]?.[key]) {
      return i18n[targetLang][key]
    }
  }
  return text
}
