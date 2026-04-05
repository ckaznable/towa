<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useDashboardFilters } from './lib/dashboardFilters'
import { api } from './lib/api'

const router = useRouter()
const route = useRoute()
const { sources, streamArticles, syncStreamArticleReadState } = useDashboardFilters()
const markingAllRead = ref(false)

const selectedSourceId = computed(() =>
  typeof route.query.source === 'string' ? route.query.source : null,
)

const showSourceFilters = computed(
  () =>
    route.name === 'dashboard' &&
    route.query.view !== 'favorites' &&
    route.query.view !== 'bookmarks',
)

const sourceUnreadCounts = computed(() => {
  const counts = new Map<string, number>()
  for (const article of streamArticles.value) {
    if (article.llm_status !== 'done' || article.read) continue
    counts.set(article.source_id, (counts.get(article.source_id) ?? 0) + 1)
  }
  return counts
})

const allSourcesUnreadCount = computed(() =>
  Array.from(sourceUnreadCounts.value.values()).reduce((sum, count) => sum + count, 0),
)

const unreadArticlesInSelection = computed(() =>
  streamArticles.value.filter((article) => {
    if (article.llm_status !== 'done' || article.read) return false
    if (!selectedSourceId.value) return true
    return article.source_id === selectedSourceId.value
  }),
)

function goToDashboard() {
  router.push({ name: 'dashboard' })
}

function goToArchive() {
  router.push({ name: 'dashboard', query: { view: 'favorites' } })
}

function goToSettings() {
  router.push({ name: 'settings' })
}

function selectSource(sourceId: string | null) {
  router.push({
    name: 'dashboard',
    query: sourceId ? { source: sourceId } : {},
  })
}

function sourceUnreadCount(sourceId: string) {
  return sourceUnreadCounts.value.get(sourceId) ?? 0
}

async function markVisibleArticlesRead() {
  if (markingAllRead.value || unreadArticlesInSelection.value.length === 0) return

  markingAllRead.value = true
  try {
    const results = await Promise.allSettled(
      unreadArticlesInSelection.value.map((article) => api.setReadState(article.id, true)),
    )

    for (const result of results) {
      if (result.status !== 'fulfilled') continue
      syncStreamArticleReadState(result.value.id, true, result.value.read_at)
    }
  } finally {
    markingAllRead.value = false
  }
}
</script>

<template>
  <div class="dashboard-container">
    <!-- Sidebar: Navigation and Branding (Global) -->
    <aside class="sidebar">
      <div class="brand-section">
        <h1 class="brand-name">永遠</h1>
      </div>
      
      <nav class="nav-menu">
        <button class="nav-item" :class="{ active: route.name === 'dashboard' && !route.query.view }" @click="goToDashboard">
          Dashboard
        </button>
        <button class="nav-item" :class="{ active: route.query.view === 'favorites' || route.query.view === 'bookmarks' }" @click="goToArchive">
          Favorites
        </button>
        <button class="nav-item" :class="{ active: route.name === 'settings' }" @click="goToSettings">
          Settings
        </button>
      </nav>

      <section v-if="showSourceFilters" class="sidebar-section">
        <p class="kicker">Filters</p>
        <div class="sidebar-source-list">
          <button class="sidebar-source-item" :class="{ active: selectedSourceId === null }" @click="selectSource(null)">
            <span class="sidebar-source-title">All feeds</span>
            <span class="sidebar-source-count">{{ allSourcesUnreadCount }}</span>
          </button>
          <button
            v-for="source in sources"
            :key="source.id"
            class="sidebar-source-item"
            :class="{ active: selectedSourceId === source.id }"
            @click="selectSource(source.id)"
          >
            <span class="sidebar-source-title">{{ source.title }}</span>
            <span class="sidebar-source-count">{{ sourceUnreadCount(source.id) }}</span>
          </button>
        </div>
        <button
          class="sidebar-action-button"
          :disabled="markingAllRead || unreadArticlesInSelection.length === 0"
          @click="markVisibleArticlesRead"
        >
          {{ markingAllRead ? 'Marking...' : 'Mark All Read' }}
        </button>
      </section>

      <div style="margin-top: auto;">
        <!-- Space for branding or future minimalist info -->
      </div>
    </aside>

    <!-- Routed Content -->
    <router-view />

    <!-- Global FAB -->
    <button v-if="route.name !== 'settings'" class="btn-fab" @click="goToSettings">
      <span style="font-size: 1.5rem;">+</span>
    </button>
  </div>
</template>
