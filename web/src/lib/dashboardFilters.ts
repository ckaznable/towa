import { computed, ref } from 'vue'
import { api } from './api'
import type { ArticleListItem, Source } from './types'

const sources = ref<Source[]>([])
const unreadCounts = ref<Record<string, number>>({})
const unreadTotal = computed(() =>
  Object.values(unreadCounts.value).reduce((sum, count) => sum + count, 0),
)
const streamRevision = ref(0)

async function refreshSources() {
  sources.value = await api.listSources()
  return sources.value
}

async function refreshUnreadCounts() {
  const payload = await api.listUnreadCounts()
  unreadCounts.value = Object.fromEntries(
    payload.items.map((item) => [item.source_id, item.unread]),
  )
  return payload
}

async function refreshDashboardFilters() {
  const [nextSources, unreadPayload] = await Promise.all([
    refreshSources(),
    refreshUnreadCounts(),
  ])

  return {
    sources: nextSources,
    unreadCounts: unreadPayload.items,
    unreadTotal: unreadPayload.total_unread,
  }
}

function sourceUnreadCount(sourceId: string) {
  return unreadCounts.value[sourceId] ?? 0
}

function syncStreamArticleReadState(
  article: Pick<ArticleListItem, 'source_id' | 'llm_status' | 'read'>,
  read: boolean,
) {
  if (article.llm_status !== 'done' || article.read === read) {
    return
  }

  const current = sourceUnreadCount(article.source_id)
  unreadCounts.value = {
    ...unreadCounts.value,
    [article.source_id]: read ? Math.max(0, current - 1) : current + 1,
  }
}

function invalidateStreamArticles() {
  streamRevision.value += 1
}

export function useDashboardFilters() {
  return {
    sources,
    unreadCounts,
    unreadTotal,
    streamRevision,
    refreshSources,
    refreshUnreadCounts,
    refreshDashboardFilters,
    sourceUnreadCount,
    syncStreamArticleReadState,
    invalidateStreamArticles,
  }
}
