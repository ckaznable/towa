<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { useDashboardFilters } from '../lib/dashboardFilters'
import { api } from '../lib/api'
import { renderMarkdown } from '../lib/markdown'
import type {
  ArticleDetail,
  ArticleListItem,
  ArticleListResponse,
  ProcessingStatus,
} from '../lib/types'

const route = useRoute()
const PAGE_SIZE = 50

type ViewMode = 'stream' | 'favorites'

const articlePage = ref<ArticleListResponse>(emptyArticlePage())
const articleDetail = ref<ArticleDetail | null>(null)
const selectedArticleId = ref<string | null>(null)
const currentPage = ref(1)
const unreadOnly = ref(false)
const {
  sources,
  refreshDashboardFilters,
  unreadTotal,
  sourceUnreadCount,
  syncStreamArticleReadState,
  streamRevision,
} = useDashboardFilters()

const currentView = computed<ViewMode>(() => {
  const view = route.query.view
  return view === 'favorites' || view === 'bookmarks' ? 'favorites' : 'stream'
})
const selectedSourceId = computed(() =>
  typeof route.query.source === 'string' ? route.query.source : null,
)
const notice = ref('')
const loadingState = reactive({
  booting: true,
  refreshing: false,
})

const sourceMap = computed(() => new Map(sources.value.map((source) => [source.id, source])))
const activeSource = computed(() => {
  if (!selectedSourceId.value) return null
  return sourceMap.value.get(selectedSourceId.value) ?? null
})

const currentCollectionLabel = computed(() => {
  if (currentView.value === 'favorites') return 'Favorites'
  return activeSource.value?.title ?? 'All feeds'
})

const articleEmptyTitle = computed(() => {
  if (currentView.value === 'favorites') return 'No saved stories yet'
  return 'No stories in this lane'
})

const articles = computed(() => articlePage.value.items)
const pageCount = computed(() =>
  articlePage.value.total === 0 ? 1 : Math.ceil(articlePage.value.total / articlePage.value.limit),
)
const pageStart = computed(() => {
  if (articlePage.value.total === 0) return 0
  return articlePage.value.offset + 1
})
const pageEnd = computed(() => articlePage.value.offset + articlePage.value.items.length)
const hasPreviousPage = computed(() => currentPage.value > 1)
const hasNextPage = computed(() => articlePage.value.has_more)
const unreadCount = computed(() => {
  if (currentView.value === 'stream') {
    return selectedSourceId.value ? sourceUnreadCount(selectedSourceId.value) : unreadTotal.value
  }
  return articles.value.filter((article) => !article.read).length
})

const visibleArticles = computed(() => articles.value)

const unreadLabel = computed(() =>
  currentView.value === 'favorites' ? 'Unread' : `Unread ${unreadCount.value}`,
)

onMounted(async () => {
  await bootstrap()
})

watch(
  () => [route.query.view, route.query.source],
  () => {
    currentPage.value = 1
    selectedArticleId.value = null
    void refreshArticles(false)
  },
)

watch(unreadOnly, () => {
  currentPage.value = 1
  selectedArticleId.value = null
  void refreshArticles(false)
})

watch(streamRevision, () => {
  if (currentView.value === 'stream') {
    void refreshArticles(true)
  }
})

async function bootstrap() {
  loadingState.booting = true
  try {
    await refreshDashboardFilters()
    await refreshArticles(false)
  } catch (error) {
    setNotice(error)
  } finally {
    loadingState.booting = false
  }
}

async function refreshDashboard() {
  loadingState.refreshing = true
  notice.value = ''
  try {
    await refreshDashboardFilters()
    await refreshArticles(true)
  } catch (error) {
    setNotice(error)
  } finally {
    loadingState.refreshing = false
  }
}

async function refreshArticles(preserveSelection: boolean) {
  const nextPage = await fetchArticlePage()
  if (nextPage.items.length === 0 && nextPage.total > 0 && currentPage.value > 1) {
    currentPage.value = Math.max(1, Math.ceil(nextPage.total / nextPage.limit))
    if (currentPage.value !== pageFromOffset(nextPage.offset, nextPage.limit)) {
      await refreshArticles(preserveSelection)
      return
    }
  }
  articlePage.value = nextPage

  const nextId =
    preserveSelection &&
    selectedArticleId.value &&
    nextPage.items.some((article) => article.id === selectedArticleId.value)
      ? selectedArticleId.value
      : (nextPage.items[0]?.id ?? null)

  selectedArticleId.value = nextId
  if (nextId) await loadArticleDetail(nextId, false)
  else articleDetail.value = null
}

async function fetchArticlePage() {
  const offset = (currentPage.value - 1) * PAGE_SIZE
  const params = {
    limit: PAGE_SIZE,
    offset,
    llmStatus: 'done' as const,
    read: unreadOnly.value ? false : undefined,
  }

  if (currentView.value === 'favorites') {
    return api.listFavorites(params)
  }

  return api.listArticles({
    ...params,
    sourceId: selectedSourceId.value,
  })
}

async function loadArticleDetail(articleId: string, markAsRead: boolean) {
  try {
    articleDetail.value = markAsRead
      ? await api.setReadState(articleId, true)
      : await api.getArticle(articleId)
    if (markAsRead) {
      syncArticleReadState(articleId, true, articleDetail.value.read_at)
    }
  } catch (error) {
    setNotice(error)
  }
}

async function selectArticle(articleId: string) {
  selectedArticleId.value = articleId
  await loadArticleDetail(articleId, true)
}

async function handleArticleClick(article: ArticleListItem) {
  if (isTitleOnly(article)) {
    openArticleInNewTab(article.url)
    if (!article.read) {
      try {
        const detail = await api.setReadState(article.id, true)
        syncArticleReadState(article.id, true, detail.read_at)
      } catch (error) {
        setNotice(error)
      }
    }
    return
  }

  await selectArticle(article.id)
}

function setNotice(error: unknown) {
  notice.value = error instanceof Error ? error.message : String(error)
}

function formatTimestamp(value: string | null) {
  if (!value) return 'not scheduled'
  return new Intl.DateTimeFormat('zh-TW', {
    dateStyle: 'medium',
    timeStyle: 'short',
  }).format(new Date(value))
}

function displayTimestamp(article: Pick<ArticleListItem, 'published_at' | 'available_at'>) {
  return formatTimestamp(article.published_at ?? article.available_at)
}

function displayDetailTimestamp(article: Pick<ArticleDetail, 'published_at' | 'available_at'>) {
  return formatTimestamp(article.published_at ?? article.available_at)
}

function processingLabel(status: ProcessingStatus) {
  switch (status) {
    case 'pending': return 'Queued'
    case 'processing': return 'Processing'
    case 'done': return 'Ready'
    case 'failed': return 'Failed'
  }
}

function isRead(articleId: string) {
  return articles.value.find((article) => article.id === articleId)?.read ?? false
}

function isTitleOnly(article: ArticleListItem) {
  return article.content.trim().length === 0 && article.summary.trim().length === 0
}

function openArticleInNewTab(url: string) {
  window.open(url, '_blank', 'noopener,noreferrer')
}

function syncArticleReadState(articleId: string, read: boolean, readAt: string | null) {
  const previous = articlePage.value.items.find((article) => article.id === articleId)
  if (previous) {
    syncStreamArticleReadState(previous, read)
  }

  articlePage.value = {
    ...articlePage.value,
    items: articlePage.value.items.map((article) =>
      article.id === articleId ? { ...article, read, read_at: readAt } : article,
    ),
  }
}

function emptyArticlePage(): ArticleListResponse {
  return {
    items: [],
    total: 0,
    limit: PAGE_SIZE,
    offset: 0,
    has_more: false,
  }
}

function pageFromOffset(offset: number, limit: number) {
  return Math.floor(offset / limit) + 1
}

async function goToPreviousPage() {
  if (!hasPreviousPage.value) return
  currentPage.value -= 1
  await refreshArticles(false)
}

async function goToNextPage() {
  if (!hasNextPage.value) return
  currentPage.value += 1
  await refreshArticles(false)
}

function syncArticleFavoriteState(articleId: string, favorited: boolean) {
  articlePage.value = {
    ...articlePage.value,
    items: articlePage.value.items.map((article) =>
      article.id === articleId ? { ...article, favorited, bookmarked: favorited } : article,
    ),
  }

  if (articleDetail.value?.id === articleId) {
    articleDetail.value = {
      ...articleDetail.value,
      favorited,
      bookmarked: favorited,
    }
  }
}

async function toggleFavorite(articleId: string, favorited: boolean) {
  notice.value = ''
  try {
    await api.setFavorite(articleId, !favorited)
    syncArticleFavoriteState(articleId, !favorited)
    if (currentView.value === 'favorites' && favorited) {
      await refreshArticles(true)
    }
  } catch (error) {
    setNotice(error)
  }
}

const articleBodyHtml = computed(() =>
  articleDetail.value
    ? renderMarkdown(
        articleDetail.value.llm_summary ??
        articleDetail.value.content ??
        articleDetail.value.summary,
      )
    : '',
)
</script>

<template>
  <!-- Feed List: Scrollable list of articles -->
  <section class="feed-list">
    <header class="feed-header">
      <div style="min-width: 0;">
        <p class="kicker">{{ currentView === 'favorites' ? 'Favorites' : 'Intelligence' }}</p>
        <h2 class="serif-text">{{ currentCollectionLabel.toUpperCase() }}</h2>
      </div>
      <div style="display: flex; gap: 0.5rem; align-items: center;">
        <button class="btn-compact" :class="{ active: unreadOnly }" @click="unreadOnly = !unreadOnly">
          {{ unreadLabel }}
        </button>
        <button class="btn-compact" @click="refreshDashboard" :disabled="loadingState.refreshing">
          {{ loadingState.refreshing ? '...' : 'Refresh' }}
        </button>
      </div>
    </header>

    <p v-if="notice" class="feed-notice">{{ notice }}</p>

    <div class="feed-items-container">
      <div v-if="!visibleArticles.length" class="empty-panel" style="min-height: auto; padding: 3rem 1rem;">
        <p style="font-size: 0.9rem;">{{ articleEmptyTitle }}</p>
      </div>

      <article 
        v-for="article in visibleArticles" 
        :key="article.id" 
        class="feed-item" 
        :class="{ active: selectedArticleId === article.id }"
        @click="handleArticleClick(article)"
      >
        <div class="item-meta">
          <span class="item-source">{{ article.source_title }}</span>
          <span>•</span>
          <span class="item-time">{{ displayTimestamp(article) }}</span>
          <span v-if="!isRead(article.id)" class="item-unread">Unread</span>
        </div>
        <h3 class="item-title serif-text">{{ article.llm_title ?? article.title }}</h3>
        <div class="item-secondary-row">
          <span v-if="isTitleOnly(article)" class="item-title-only">Title only · open original</span>
        </div>
        <div style="display: flex; justify-content: flex-end; margin-top: 0.5rem;">
          <span :class="['status-pill', `is-${article.llm_status}`]">
            {{ processingLabel(article.llm_status) }}
          </span>
        </div>
      </article>
    </div>

    <footer v-if="articlePage.total > 0" class="feed-pagination">
      <span class="feed-pagination-summary">
        {{ pageStart }}-{{ pageEnd }} / {{ articlePage.total }}
      </span>
      <div class="feed-pagination-controls">
        <button class="btn-compact" :disabled="!hasPreviousPage" @click="goToPreviousPage">
          Prev
        </button>
        <span class="feed-pagination-page">{{ currentPage }} / {{ pageCount }}</span>
        <button class="btn-compact" :disabled="!hasNextPage" @click="goToNextPage">
          Next
        </button>
      </div>
    </footer>
  </section>

  <!-- Article Preview: Detailed view of the selected article -->
  <main class="article-preview">
    <header class="article-toolbar">
      <div style="display: flex; gap: 1rem;">
        <button v-if="articleDetail" class="nav-item" @click="toggleFavorite(articleDetail.id, articleDetail.favorited)">
          {{ articleDetail.favorited ? 'Remove favorite' : 'Add favorite' }}
        </button>
      </div>
      <div v-if="articleDetail">
        <a :href="articleDetail.url" target="_blank" class="btn-primary" style="text-decoration: none;">Visit Source</a>
      </div>
    </header>

    <div v-if="articleDetail" class="content-area">
      <div class="article-meta">
        {{ articleDetail.source_title }} • {{ displayDetailTimestamp(articleDetail) }}
      </div>
      <h1 class="article-title serif-text">{{ articleDetail.llm_title ?? articleDetail.title }}</h1>

      <div class="article-body serif-text">
        <div
          v-if="articleDetail.llm_summary || articleDetail.summary"
          class="article-markdown"
          v-html="articleBodyHtml"
        />

        <div v-if="articleDetail.llm_error" class="key-entities" style="background-color: rgba(255, 110, 132, 0.08); margin-top: 4rem; padding: 2rem; border-radius: 24px;">
          <p class="kicker" style="color: var(--error); margin-bottom: 1rem;">Intelligence Synthesis Error</p>
          <p style="color: var(--error); line-height: 1.6;">{{ articleDetail.llm_error }}</p>
        </div>

      </div>
    </div>

    <div v-else class="content-area" style="display: grid; place-items: center; text-align: center;">
      <div>
        <p class="kicker">Reader Ready</p>
        <h1 class="article-title serif-text" style="font-size: 2rem; margin-top: 1rem;">SELECT AN ENTRY</h1>
        <p style="color: var(--text-muted); max-width: 400px; margin: 1rem auto;">Choose a dispatch from the intelligence feed to begin full context synthesis.</p>
      </div>
    </div>
  </main>
</template>
