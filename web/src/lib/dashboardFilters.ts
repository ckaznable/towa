import { ref } from 'vue'
import { api } from './api'
import type { ArticleListItem, Source } from './types'

const sources = ref<Source[]>([])
const streamArticles = ref<ArticleListItem[]>([])

async function refreshSources() {
  sources.value = await api.listSources()
  return sources.value
}

async function refreshStreamArticles() {
  streamArticles.value = await api.listArticles()
  return streamArticles.value
}

async function refreshDashboardFilters() {
  const [nextSources, nextStreamArticles] = await Promise.all([
    refreshSources(),
    refreshStreamArticles(),
  ])

  return {
    sources: nextSources,
    streamArticles: nextStreamArticles,
  }
}

function syncStreamArticleReadState(articleId: string, read: boolean, readAt: string | null) {
  streamArticles.value = streamArticles.value.map((article) =>
    article.id === articleId ? { ...article, read, read_at: readAt } : article,
  )
}

export function useDashboardFilters() {
  return {
    sources,
    streamArticles,
    refreshSources,
    refreshStreamArticles,
    refreshDashboardFilters,
    syncStreamArticleReadState,
  }
}
