export const i18n = {
  'zh-CN': {
    settingsTitle: '设置', settingsTheme: '主题', themeDark: '深色', themeLight: '浅色',
    settingsLang: '语言', btnClose: '关闭',
    btnOpen: '打开文件', btnOpenMenu: '打开', btnPreviewPdf: '预览 PDF', btnBack: '返回预览', btnSave: '保存 PDF', navBack: '后退', navForward: '前进',
    btnExport: '导出', exportHtml: 'HTML', exportPdf: 'PDF', exportDocx: 'DOCX',
    statusExporting: '正在导出…', statusExportReady: '导出完成', statusExportError: '导出失败',
    welcomeTitle: '准备就绪', welcomeSub: '打开一个 Markdown 文档，即可预览并导出为 PDF、HTML、DOCX 等多种格式',
    welcomeFileHint: '点击顶部「打开」选文件，或直接把 .md 文件拖入窗口',
    welcomeFolderHint: '在「打开」菜单中选择文件夹，或直接把文件夹拖入窗口',
    statusReady: '就绪', statusGenerating: '正在生成预览…',
    statusPreviewReady: '预览已生成', statusGeneratingPdf: '正在生成 PDF…',
    statusPdfReady: 'PDF 已生成', statusSaving: '正在保存…', statusPdfSaved: 'PDF 已保存',
    statusNoPdf: '没有可保存的 PDF',
    aboutDesc: 'Markdown 一键预览，导出 PDF、HTML、DOCX 等多种格式',
    aboutTech: '基于 Tauri v2 · Chrome 无头渲染',
    pdfOverlayPreparing: '正在准备 PDF 预览…',
    brandHint: 'Markdown 转 PDF、HTML、DOCX', btnFullWidth: '全宽显示',
    hintMac: '在 Finder 中将 .md 设为默认打开程序：<br>右键 .md 文件 → 显示简介 → 打开方式 → md2x → 全部更改',
    hintWin: '在资源管理器中将 .md 设为默认打开程序：<br>右键 .md 文件 → 打开方式 → 选择其他应用 → md2x → 始终使用',
    apiUnavailable: 'Tauri API 不可用',
    btnOpenFolder: '打开文件夹', fileTreeHide: '隐藏文件树', fileTreeShow: '显示文件树', treeNoMarkdown: '此文件夹中没有 Markdown 文件',
    settingsView: '默认视图',
    viewSingle: '单文件视图', viewSingleDesc: '启动时不显示文件树',
    viewFolder: '文件夹视图', viewFolderDesc: '打开文件时自动显示其所在文件夹的文件树',
    viewAuto: '自动', viewAutoDesc: '按启动时打开的是文件还是文件夹自动选择',
    viewNote: '更改将在下次启动时生效',
    settingsDefaultApp: '默认打开程序',
    defaultAppDesc: '将 .md 关联到 md2x，双击 Markdown 文件即可直接预览',
    btnSetDefault: '设为默认 .md 打开程序',
    defaultSetOkMac: '已通过 LaunchServices 注册。若未立即生效，请在 Finder 中右键任一 .md → 显示简介 → 打开方式 → md2x → 全部更改。',
    defaultSetOkWin: '注册成功。若系统仍用其他应用打开 .md，请右键 .md → 打开方式 → 选择其他应用 → md2x → 始终使用。',
    defaultSetOkLinux: '已通过 xdg-mime 注册，可能需要重新登录后完全生效。',
    defaultSetFail: '设置失败：',
    defaultAppNote: 'Windows 写入注册表 · macOS 写入 LaunchServices · Linux 使用 xdg-mime，均为当前用户级设置',
  },
  'en': {
    settingsTitle: 'Settings', settingsTheme: 'Theme', themeDark: 'Dark', themeLight: 'Light',
    settingsLang: 'Language', btnClose: 'Close',
    btnOpen: 'Open File', btnOpenMenu: 'Open', btnPreviewPdf: 'Preview PDF', btnBack: '← Back', btnSave: 'Save PDF', navBack: 'Back', navForward: 'Forward',
    btnExport: 'Export', exportHtml: 'HTML', exportPdf: 'PDF', exportDocx: 'DOCX',
    statusExporting: 'Exporting…', statusExportReady: 'Exported', statusExportError: 'Export failed',
    welcomeTitle: 'Ready', welcomeSub: 'Open a Markdown file to preview and export as PDF, HTML, or DOCX',
    welcomeFileHint: 'Pick a file from \u201cOpen\u201d at the top, or drop a .md file into the window',
    welcomeFolderHint: 'Pick a folder from the \u201cOpen\u201d menu at the top, or drop a folder into the window',
    statusReady: 'Ready', statusGenerating: 'Generating preview…',
    statusPreviewReady: 'Preview ready', statusGeneratingPdf: 'Generating PDF…',
    statusPdfReady: 'PDF ready', statusSaving: 'Saving…', statusPdfSaved: 'PDF saved',
    statusNoPdf: 'No PDF to save',
    aboutDesc: 'Instant Markdown preview with export to PDF, HTML, and DOCX',
    aboutTech: 'Powered by Tauri v2 · Chrome headless',
    pdfOverlayPreparing: 'Preparing PDF preview…',
    brandHint: 'Markdown to PDF, HTML, or DOCX', btnFullWidth: 'Full width',
    hintMac: 'Set .md as default in Finder:<br>Right-click .md → Get Info → Open with → md2x → Change All',
    hintWin: 'Set .md as default in File Explorer:<br>Right-click .md → Open with → Choose another app → md2x → Always use',
    apiUnavailable: 'Tauri API unavailable',
    btnOpenFolder: 'Open Folder', fileTreeHide: 'Hide file tree', fileTreeShow: 'Show file tree', treeNoMarkdown: 'No Markdown files in this folder',
    settingsView: 'Default view',
    viewSingle: 'Single file', viewSingleDesc: 'No file tree at launch',
    viewFolder: 'Folder tree', viewFolderDesc: 'Auto show the file tree of the file\u2019s folder when opening a file',
    viewAuto: 'Auto', viewAutoDesc: 'Choose based on whether a file or a folder is opened at launch',
    viewNote: 'Takes effect on next launch',
    settingsDefaultApp: 'Default app',
    defaultAppDesc: 'Associate .md with md2x so double-clicking a Markdown file opens it here',
    btnSetDefault: 'Set as default .md opener',
    defaultSetOkMac: 'Registered via LaunchServices. If it does not apply right away, right-click a .md in Finder → Get Info → Open with → md2x → Change All.',
    defaultSetOkWin: 'Registered. If another app still opens .md, right-click .md → Open with → Choose another app → md2x → Always.',
    defaultSetOkLinux: 'Registered via xdg-mime; it may fully take effect after re-login.',
    defaultSetFail: 'Setup failed: ',
    defaultAppNote: 'Windows writes the registry · macOS writes LaunchServices · Linux uses xdg-mime. All per-user settings',
  },
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
