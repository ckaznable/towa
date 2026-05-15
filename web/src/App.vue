<script setup lang="ts">
import { computed, ref } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { useDashboardFilters } from './lib/dashboardFilters'
import { api } from './lib/api'

const router = useRouter()
const route = useRoute()
const {
  sources,
  unreadCounts,
  unreadTotal,
  refreshUnreadCounts,
  invalidateStreamArticles,
} = useDashboardFilters()
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

const sourceUnreadCounts = computed(() => unreadCounts.value)
const allSourcesUnreadCount = computed(() => unreadTotal.value)

const sortedSources = computed(() =>
  [...sources.value].sort((left, right) => {
    const unreadDelta = sourceUnreadCount(right.id) - sourceUnreadCount(left.id)
    if (unreadDelta !== 0) return unreadDelta
    return left.title.localeCompare(right.title, 'zh-TW')
  }),
)

const unreadArticlesInSelectionCount = computed(() =>
  selectedSourceId.value ? sourceUnreadCount(selectedSourceId.value) : allSourcesUnreadCount.value,
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
  return sourceUnreadCounts.value[sourceId] ?? 0
}

function sourceCountClass(count: number) {
  if (count === 0) return 'is-empty'
  if (count < 5) return 'is-low'
  if (count < 10) return 'is-medium'
  return 'is-high'
}

async function markVisibleArticlesRead() {
  if (markingAllRead.value || unreadArticlesInSelectionCount.value === 0) return

  markingAllRead.value = true
  try {
    await api.markSelectionRead(selectedSourceId.value)
    await refreshUnreadCounts()
    invalidateStreamArticles()
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
            <span class="sidebar-source-count" :class="sourceCountClass(allSourcesUnreadCount)">
              {{ allSourcesUnreadCount }}
            </span>
          </button>
          <button
            v-for="source in sortedSources"
            :key="source.id"
            class="sidebar-source-item"
            :class="{ active: selectedSourceId === source.id }"
            @click="selectSource(source.id)"
          >
            <span class="sidebar-source-title">{{ source.title }}</span>
            <span class="sidebar-source-count" :class="sourceCountClass(sourceUnreadCount(source.id))">
              {{ sourceUnreadCount(source.id) }}
            </span>
          </button>
        </div>
        <button
          class="sidebar-action-button"
          :disabled="markingAllRead || unreadArticlesInSelectionCount === 0"
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
