<script setup lang="ts">
import { useRouter, useRoute } from 'vue-router'

const router = useRouter()
const route = useRoute()

function goToDashboard() {
  router.push({ name: 'dashboard' })
}

function goToArchive() {
  router.push({ name: 'dashboard', query: { view: 'favorites' } })
}

function goToSettings() {
  router.push({ name: 'settings' })
}
</script>

<template>
  <div class="dashboard-container">
    <!-- Sidebar: Navigation and Branding (Global) -->
    <aside class="sidebar">
      <div class="brand-section">
        <h1 class="brand-name">TOWA</h1>
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
