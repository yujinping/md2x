// 注入到预览 iframe HTML 中的运行时：页内查找 + 黄底高亮。
// 与后端渲染产物隔离，仅用于应用内预览（导出文件不含此脚本）。

const FIND_CSS = `<style id="md2x-find-style">
mark.md2x-find-hit{background:#ffe066!important;color:#1a1400!important;padding:0 1px;border-radius:2px;box-shadow:none}
mark.md2x-find-active{background:#ff9f1c!important;color:#1a1400!important}
</style>`

const FIND_SCRIPT = `
(function () {
  var hits = [];
  var active = -1;

  function toParent(msg) {
    if (window.parent && window.parent !== window) window.parent.postMessage(msg, '*');
  }

  function clearHits() {
    for (var i = 0; i < hits.length; i++) {
      var m = hits[i];
      var p = m.parentNode;
      if (!p) continue;
      while (m.firstChild) p.insertBefore(m.firstChild, m);
      p.removeChild(m);
      p.normalize();
    }
    hits = [];
    active = -1;
  }

  function markNode(node, text, hay, needle) {
    var idx = hay.indexOf(needle);
    if (idx < 0) return;
    var frag = document.createDocumentFragment();
    var last = 0;
    while (idx >= 0) {
      if (idx > last) frag.appendChild(document.createTextNode(text.slice(last, idx)));
      var m = document.createElement('mark');
      m.className = 'md2x-find-hit';
      m.textContent = text.slice(idx, idx + needle.length);
      frag.appendChild(m);
      hits.push(m);
      last = idx + needle.length;
      idx = hay.indexOf(needle, last);
    }
    if (last < text.length) frag.appendChild(document.createTextNode(text.slice(last)));
    node.parentNode.replaceChild(frag, node);
  }

  function search(query) {
    clearHits();
    if (!query || !document.body) return;
    var needle = query.toLowerCase();
    var walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, function (node) {
      var p = node.parentElement;
      if (!p || !node.nodeValue) return NodeFilter.FILTER_REJECT;
      // 只标记 HTML 命名空间的文本：往 SVG 图形（mermaid）里插 <mark> 会破坏绘图
      if (p.namespaceURI !== 'http://www.w3.org/1999/xhtml') return NodeFilter.FILTER_REJECT;
      var tag = p.tagName.toUpperCase();
      if (tag === 'SCRIPT' || tag === 'STYLE' || tag === 'NOSCRIPT' || tag === 'TEXTAREA') {
        return NodeFilter.FILTER_REJECT;
      }
      return NodeFilter.FILTER_ACCEPT;
    }, false);
    var nodes = [];
    while (walker.nextNode()) nodes.push(walker.currentNode);
    for (var i = 0; i < nodes.length; i++) {
      var text = nodes[i].nodeValue;
      markNode(nodes[i], text, text.toLowerCase(), needle);
    }
  }

  function setActive(next) {
    if (!hits.length) { active = -1; return; }
    var prev = hits[active];
    if (prev) prev.classList.remove('md2x-find-active');
    active = ((next % hits.length) + hits.length) % hits.length;
    var el = hits[active];
    el.classList.add('md2x-find-active');
    el.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }

  function reply() {
    toParent({ type: 'md2x-find-result', total: hits.length, index: active + 1 });
  }

  window.addEventListener('message', function (e) {
    var d = e.data;
    if (!d || d.type !== 'md2x-find') return;
    if (d.action === 'clear') { clearHits(); reply(); return; }
    if (d.action === 'next') { setActive(active + 1); reply(); return; }
    if (d.action === 'prev') { setActive(active - 1); reply(); return; }
    var q = d.query || '';
    if (!q) { clearHits(); reply(); return; }
    search(q);
    if (hits.length) setActive(0);
    reply();
  });

  // 焦点在正文内时，⌘/Ctrl+F 交由父窗口弹出查找栏
  document.addEventListener('keydown', function (e) {
    if ((e.metaKey || e.ctrlKey) && !e.shiftKey && (e.key === 'f' || e.key === 'F')) {
      e.preventDefault();
      toParent({ type: 'md2x-find-open' });
    }
  }, true);
})();
`

/// 把查找运行时（样式 + 脚本）注入渲染后的 HTML
export function injectFindRuntime(html) {
  const open = '<' + 'script>'
  const close = '<' + '/script>'
  const payload = FIND_CSS + open + FIND_SCRIPT + close
  // 取最后一个 </body>：正文脚本里若出现过该字面量，插入点仍在真正的文档末尾
  const at = html.lastIndexOf('</body>')
  if (at !== -1) return html.slice(0, at) + payload + html.slice(at)
  return html + payload
}
