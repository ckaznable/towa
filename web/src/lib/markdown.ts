function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;')
    .replaceAll("'", '&#39;')
}

function normalizeHtmlBreaks(value: string): string {
  return value
    .replaceAll('\r\n', '\n')
    .replace(/<br\s*\/?>/gi, '\n')
    .replace(/<\/p>\s*<p[^>]*>/gi, '\n\n')
    .replace(/<p[^>]*>/gi, '')
    .replace(/<\/p>/gi, '\n\n')
}

function sanitizeUrl(value: string): string {
  const trimmed = value.trim()
  if (/^(https?:|mailto:)/i.test(trimmed)) {
    return trimmed
  }
  return '#'
}

function tokenizeCodeSpans(value: string): { text: string; tokens: string[] } {
  const tokens: string[] = []
  const text = value.replace(/`([^`]+)`/g, (_, code: string) => {
    const index = tokens.push(`<code>${escapeHtml(code)}</code>`) - 1
    return `@@CODE_SPAN_${index}@@`
  })

  return { text, tokens }
}

function restoreCodeSpans(value: string, tokens: string[]): string {
  return value.replace(/@@CODE_SPAN_(\d+)@@/g, (_, index: string) => tokens[Number(index)] ?? '')
}

function renderInline(value: string): string {
  const { text, tokens } = tokenizeCodeSpans(escapeHtml(value))

  let rendered = text
    .replace(
      /\[([^\]]+)\]\(([^)]+)\)/g,
      (_, label: string, href: string) =>
        `<a href="${escapeHtml(sanitizeUrl(href))}" target="_blank" rel="noopener noreferrer">${label}</a>`,
    )
    .replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
    .replace(/__([^_]+)__/g, '<strong>$1</strong>')
    .replace(/(^|[^\*])\*([^*]+)\*(?!\*)/g, '$1<em>$2</em>')
    .replace(/(^|[^_])_([^_]+)_(?!_)/g, '$1<em>$2</em>')

  rendered = restoreCodeSpans(rendered, tokens)
  return rendered
}

function isHorizontalRule(line: string): boolean {
  return /^(?:\*{3,}|-{3,}|_{3,})$/.test(line.trim())
}

export function renderMarkdown(source: string): string {
  const lines = normalizeHtmlBreaks(source).split('\n')
  const blocks: string[] = []
  let index = 0

  while (index < lines.length) {
    const rawLine = lines[index]
    const line = rawLine.trim()

    if (!line) {
      index += 1
      continue
    }

    if (rawLine.startsWith('```')) {
      const codeLines: string[] = []
      index += 1
      while (index < lines.length && !lines[index].startsWith('```')) {
        codeLines.push(lines[index])
        index += 1
      }
      if (index < lines.length) {
        index += 1
      }
      blocks.push(`<pre><code>${escapeHtml(codeLines.join('\n'))}</code></pre>`)
      continue
    }

    const heading = rawLine.match(/^(#{1,6})\s+(.*)$/)
    if (heading) {
      const level = heading[1].length
      blocks.push(`<h${level}>${renderInline(heading[2].trim())}</h${level}>`)
      index += 1
      continue
    }

    if (isHorizontalRule(line)) {
      blocks.push('<hr />')
      index += 1
      continue
    }

    if (rawLine.trimStart().startsWith('>')) {
      const quoteLines: string[] = []
      while (index < lines.length) {
        const next = lines[index]
        if (!next.trim()) {
          quoteLines.push('')
          index += 1
          continue
        }
        if (!next.trimStart().startsWith('>')) {
          break
        }
        quoteLines.push(next.trimStart().replace(/^>\s?/, ''))
        index += 1
      }
      blocks.push(`<blockquote>${renderMarkdown(quoteLines.join('\n'))}</blockquote>`)
      continue
    }

    const unordered = rawLine.match(/^\s*[-*+]\s+(.*)$/)
    const ordered = rawLine.match(/^\s*\d+\.\s+(.*)$/)
    if (unordered || ordered) {
      const tag = unordered ? 'ul' : 'ol'
      const items: string[] = []
      while (index < lines.length) {
        const next = lines[index]
        const match = unordered
          ? next.match(/^\s*[-*+]\s+(.*)$/)
          : next.match(/^\s*\d+\.\s+(.*)$/)
        if (!match) {
          break
        }
        items.push(`<li>${renderInline(match[1].trim())}</li>`)
        index += 1
      }
      blocks.push(`<${tag}>${items.join('')}</${tag}>`)
      continue
    }

    const paragraphLines: string[] = []
    while (index < lines.length) {
      const next = lines[index]
      const trimmed = next.trim()
      if (
        !trimmed ||
        next.startsWith('```') ||
        /^(#{1,6})\s+/.test(next) ||
        isHorizontalRule(trimmed) ||
        next.trimStart().startsWith('>') ||
        /^\s*[-*+]\s+/.test(next) ||
        /^\s*\d+\.\s+/.test(next)
      ) {
        break
      }
      paragraphLines.push(next.trim())
      index += 1
    }

    blocks.push(`<p>${renderInline(paragraphLines.join('<br />'))}</p>`)
  }

  return blocks.join('')
}
