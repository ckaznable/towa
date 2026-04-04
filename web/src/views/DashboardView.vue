<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { api } from '../lib/api'
import type {
  AgentSummary,
  ArticleDetail,
  ArticleListItem,
  ProcessingStatus,
  Source,
} from '../lib/types'

const route = useRoute()

type ViewMode = 'stream' | 'favorites'

const sources = ref<Source[]>([])
const agents = ref<AgentSummary[]>([])
const articles = ref<ArticleListItem[]>([])
const articleDetail = ref<ArticleDetail | null>(null)
const selectedSourceId = ref<string | null>(null)
const selectedArticleId = ref<string | null>(null)
const unreadOnly = ref(false)

const currentView = computed<ViewMode>(() => {
  const view = route.query.view
  return view === 'favorites' || view === 'bookmarks' ? 'favorites' : 'stream'
})
const notice = ref('')
const loadingState = reactive({
  booting: true,
  refreshing: false,
  detail: false,
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

const readyArticles = computed(() =>
  articles.value.filter((article) => article.llm_status === 'done'),
)

const visibleArticles = computed(() =>
  readyArticles.value.filter((article) => !unreadOnly.value || !isRead(article.id)),
)

const unreadCount = computed(() =>
  readyArticles.value.filter((article) => !isRead(article.id)).length,
)

onMounted(async () => {
  await bootstrap()
})

watch(() => route.query.view, () => {
  refreshArticles(false)
})

async function bootstrap() {
  loadingState.booting = true
  try {
    await Promise.all([refreshAgents(), refreshSources()])
    await refreshArticles(false)
  } catch (error) {
    setNotice(error)
  } finally {
    loadingState.booting = false
  }
}

async function refreshAgents() {
  agents.value = await api.listAgents()
}

async function refreshSources() {
  const nextSources = await api.listSources()
  sources.value = nextSources
}

async function refreshDashboard() {
  loadingState.refreshing = true
  notice.value = ''
  try {
    await Promise.all([refreshAgents(), refreshSources()])
    await refreshArticles(true)
  } catch (error) {
    setNotice(error)
  } finally {
    loadingState.refreshing = false
  }
}

async function refreshArticles(preserveSelection: boolean) {
  const nextArticles =
    currentView.value === 'favorites'
      ? await api.listFavorites()
      : await api.listArticles(selectedSourceId.value ?? undefined)

  articles.value = nextArticles
  const nextId =
    preserveSelection &&
    selectedArticleId.value &&
    visibleArticles.value.some((article) => article.id === selectedArticleId.value)
      ? selectedArticleId.value
      : (visibleArticles.value[0]?.id ?? null)

  selectedArticleId.value = nextId
  if (nextId) await loadArticleDetail(nextId, false)
  else articleDetail.value = null
}

async function loadArticleDetail(articleId: string, markAsRead: boolean) {
  loadingState.detail = true
  try {
    articleDetail.value = markAsRead
      ? await api.setReadState(articleId, true)
      : await api.getArticle(articleId)
    if (markAsRead) {
      syncArticleReadState(articleId, true, articleDetail.value.read_at)
    }
  } catch (error) {
    setNotice(error)
  } finally {
    loadingState.detail = false
  }
}

async function selectArticle(articleId: string) {
  selectedArticleId.value = articleId
  await loadArticleDetail(articleId, true)
}

async function toggleFavorite(articleId: string, favorited: boolean) {
  notice.value = ''
  try {
    await api.setFavorite(articleId, !favorited)
    await refreshArticles(true)
  } catch (error) {
    setNotice(error)
  }
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

function syncArticleReadState(articleId: string, read: boolean, readAt: string | null) {
  articles.value = articles.value.map((article) =>
    article.id === articleId
      ? {
          ...article,
          read,
          read_at: readAt,
        }
      : article,
  )
}
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
          Unread {{ unreadCount }}
        </button>
        <button class="btn-compact" @click="refreshDashboard" :disabled="loadingState.refreshing">
          {{ loadingState.refreshing ? '...' : 'Refresh' }}
        </button>
      </div>
    </header>

    <div class="feed-items-container">
      <div v-if="!visibleArticles.length" class="empty-panel" style="min-height: auto; padding: 3rem 1rem;">
        <p style="font-size: 0.9rem;">{{ articleEmptyTitle }}</p>
      </div>

      <article 
        v-for="article in visibleArticles" 
        :key="article.id" 
        class="feed-item" 
        :class="{ active: selectedArticleId === article.id }"
        @click="selectArticle(article.id)"
      >
        <div class="item-meta">
          <span class="item-source">{{ article.source_title }}</span>
          <span>•</span>
          <span class="item-time">{{ formatTimestamp(article.available_at) }}</span>
          <span v-if="!isRead(article.id)" class="item-unread">Unread</span>
        </div>
        <h3 class="item-title serif-text">{{ article.title }}</h3>
        <div style="display: flex; justify-content: flex-end; margin-top: 0.5rem;">
          <span :class="['status-pill', `is-${article.llm_status}`]">
            {{ processingLabel(article.llm_status) }}
          </span>
        </div>
      </article>
    </div>
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

    <div v-if="loadingState.detail" class="content-area" style="display: grid; place-items: center;">
      <p class="kicker">Synthesizing intelligence...</p>
    </div>

    <div v-else-if="articleDetail" class="content-area">
      <div class="article-meta">
        {{ articleDetail.source_title }} • {{ formatTimestamp(articleDetail.available_at) }}
      </div>
      <h1 class="article-title serif-text">{{ articleDetail.title }}</h1>

      <div class="article-body serif-text">
        <section v-if="articleDetail.llm_summary" class="key-entities" style="background-color: var(--surface-bright);">
          <p style="color: var(--text-primary); line-height: 1.8;">{{ articleDetail.llm_summary }}</p>
        </section>

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
