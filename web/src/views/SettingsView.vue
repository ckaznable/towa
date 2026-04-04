<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { useRouter } from 'vue-router'
import { api } from '../lib/api'
import type {
  AgentSummary,
  Source,
} from '../lib/types'

const props = defineProps<{
  tab?: string
}>()

const router = useRouter()

type SettingsTab = 'sources' | 'agents'
const activeTab = computed<SettingsTab>(() =>
  props.tab === 'agents' ? 'agents' : 'sources',
)

const sources = ref<Source[]>([])
const agents = ref<AgentSummary[]>([])
const notice = ref('')
const loadingState = reactive({
  refreshing: false,
  savingSource: false,
})

const sourceForm = reactive({
  id: null as string | null,
  title: '',
  feedUrl: '',
  enabled: true,
  assignedAgentId: '',
})

const managerHeading = computed(() => (sourceForm.id ? 'Edit Dispatch' : 'Add Dispatch'))

onMounted(async () => {
  await Promise.all([refreshAgents(), refreshSources()])
})

async function refreshAgents() {
  agents.value = await api.listAgents()
}

async function refreshSources() {
  sources.value = await api.listSources()
}

function setTab(tab: SettingsTab) {
  router.push({ name: 'settings', params: { tab } })
}

function startEditSource(source: Source) {
  sourceForm.id = source.id
  sourceForm.title = source.title
  sourceForm.feedUrl = source.feed_url
  sourceForm.enabled = source.enabled
  sourceForm.assignedAgentId = source.assigned_agent_id ?? ''
}

function resetSourceForm() {
  sourceForm.id = null
  sourceForm.title = ''
  sourceForm.feedUrl = ''
  sourceForm.enabled = true
  sourceForm.assignedAgentId = ''
}

async function saveSource() {
  loadingState.savingSource = true
  try {
    if (sourceForm.id) {
      await api.updateSource(sourceForm.id, {
        title: sourceForm.title.trim() || undefined,
        feed_url: sourceForm.feedUrl.trim(),
        enabled: sourceForm.enabled,
      })
      await api.assignAgent(sourceForm.id, sourceForm.assignedAgentId || null)
    } else {
      await api.createSource({
        title: sourceForm.title.trim() || undefined,
        feed_url: sourceForm.feedUrl.trim(),
        enabled: sourceForm.enabled,
        assigned_agent_id: sourceForm.assignedAgentId || null,
      })
    }
    await refreshSources()
    resetSourceForm()
  } catch (error) {
    notice.value = String(error)
  } finally {
    loadingState.savingSource = false
  }
}

async function removeSource(source: Source) {
  if (!window.confirm(`Delete source "${source.title}"?`)) return
  try {
    await api.deleteSource(source.id)
    await refreshSources()
  } catch (error) {
    notice.value = String(error)
  }
}

function closeSettings() {
  router.push({ name: 'dashboard' })
}
</script>

<template>
  <div class="settings-shell" @click.self="closeSettings">
    <section class="settings-panel">
      <aside class="settings-sidebar">
        <div class="brand-section">
          <h1 class="brand-name">Settings</h1>
        </div>
        
        <nav class="nav-menu" style="gap: 0.5rem;">
          <button class="settings-nav-item" :class="{ active: activeTab === 'sources' }" @click="setTab('sources')">
            Sources
          </button>
          <button class="settings-nav-item" :class="{ active: activeTab === 'agents' }" @click="setTab('agents')">
            Agents
          </button>
        </nav>
      </aside>

      <main class="settings-content">
        <header style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 4rem;">
          <div>
            <p class="kicker">{{ activeTab.toUpperCase() }}</p>
            <h2 class="serif-text" style="font-size: 3rem; margin-top: 0.5rem; line-height: 1;">
              {{ activeTab === 'sources' ? 'Source Management' : 'Routing Intelligence' }}
            </h2>
          </div>
          <div style="display: flex; gap: 1rem;">
            <button class="settings-btn-secondary" @click="closeSettings">DISMISS</button>
            <button class="btn-primary" @click="closeSettings">SAVE CHANGES</button>
          </div>
        </header>

        <!-- Tab: Sources -->
        <div v-if="activeTab === 'sources'" class="settings-grid" style="display: grid; grid-template-columns: 1.2fr 1fr; gap: 4rem;">
          <div>
            <p class="kicker" style="margin-bottom: 2rem;">Configured Feeds</p>
            <div class="source-manager-list" style="max-height: 450px; overflow-y: auto; padding-right: 1rem;">
              <article v-for="source in sources" :key="source.id" class="source-item-card">
                <div>
                  <strong style="color: var(--text-primary); display: block;">{{ source.title }}</strong>
                  <code style="font-size: 0.7rem; color: var(--text-muted);">{{ source.feed_url }}</code>
                </div>
                <div style="display: flex; gap: 1rem;">
                  <button class="settings-btn-secondary" style="padding: 0.5rem 1rem; font-size: 0.75rem;" @click="startEditSource(source)">Edit</button>
                  <button class="settings-btn-secondary settings-btn-danger" style="padding: 0.5rem 1rem; font-size: 0.75rem;" @click="removeSource(source)">Delete</button>
                </div>
              </article>
            </div>
          </div>

          <form @submit.prevent="saveSource" class="settings-form" style="background: var(--surface-container); border-radius: 24px; padding: 2rem;">
            <p class="kicker" style="margin-bottom: 1.5rem;">{{ managerHeading }}</p>
            
            <div class="form-group">
              <label class="form-label">Display Title</label>
              <input v-model="sourceForm.title" type="text" class="form-input" placeholder="Editorial Name" />
            </div>

            <div class="form-group">
              <label class="form-label">Feed URL</label>
              <input v-model="sourceForm.feedUrl" type="url" required class="form-input" placeholder="https://..." />
            </div>

            <div class="form-group">
              <label class="form-label">Assigned Agent</label>
              <select v-model="sourceForm.assignedAgentId" class="form-select">
                <option value="">No Routing</option>
                <option v-for="agent in agents" :key="agent.id" :value="agent.id">
                  {{ agent.label }}
                </option>
              </select>
            </div>

            <div style="display: flex; justify-content: space-between; align-items: center; margin-top: 1rem; padding: 1rem; background: var(--surface-container-low); border-radius: 12px;">
              <span class="form-label">Enable Synchronization</span>
              <label class="switch">
                <input v-model="sourceForm.enabled" type="checkbox" />
                <span class="slider"></span>
              </label>
            </div>

            <div style="margin-top: 2rem; display: flex; gap: 1rem;">
              <button type="submit" class="btn-primary" style="flex: 1;" :disabled="loadingState.savingSource">
                {{ loadingState.savingSource ? 'Processing...' : 'Save Dispatch' }}
              </button>
              <button type="button" class="settings-btn-secondary" @click="resetSourceForm">Reset</button>
            </div>
          </form>
        </div>

        <!-- Tab: Agents -->
        <div v-if="activeTab === 'agents'" class="settings-section">
          <header class="settings-section-header">
            <p class="kicker">Model Infrastructure</p>
            <h3>Active Intelligence Nodes</h3>
          </header>
          <div class="source-manager-list" style="display: grid; grid-template-columns: repeat(2, 1fr); gap: 1.5rem;">
            <article v-for="agent in agents" :key="agent.id" class="source-item-card" style="padding: 2rem;">
              <div>
                <p class="kicker">{{ agent.model }}</p>
                <h3 class="serif-text" style="margin-top: 0.5rem;">{{ agent.label }}</h3>
                <div style="margin-top: 1.5rem;" class="health-indicator">
                  <span class="status-dot healthy"></span>
                  <span style="color: var(--text-secondary); font-size: 0.8rem;">Ready for Inference</span>
                </div>
              </div>
            </article>
          </div>
        </div>

      </main>
    </section>
  </div>
</template>
