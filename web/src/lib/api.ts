import type {
  AgentListResponse,
  ArticleDetail,
  ArticleListResponse,
  CreateSourcePayload,
  Source,
  SourceListResponse,
  UpdateSourcePayload,
} from './types'

const API_BASE = import.meta.env.VITE_API_BASE_URL?.replace(/\/$/, '') ?? ''

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`${API_BASE}${path}`, {
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers ?? {}),
    },
    ...init,
  })

  if (!response.ok) {
    let message = `Request failed with status ${response.status}`

    try {
      const payload = (await response.json()) as { error?: string }
      if (payload.error) {
        message = payload.error
      }
    } catch {
      // Ignore response parsing errors and keep fallback message.
    }

    throw new Error(message)
  }

  if (response.status === 204) {
    return undefined as T
  }

  return (await response.json()) as T
}

export const api = {
  async listAgents() {
    const payload = await request<AgentListResponse>('/api/agents')
    return payload.items
  },

  async listSources() {
    const payload = await request<SourceListResponse>('/api/sources')
    return payload.items
  },

  createSource(payload: CreateSourcePayload) {
    return request<Source>('/api/sources', {
      method: 'POST',
      body: JSON.stringify(payload),
    })
  },

  updateSource(id: string, payload: UpdateSourcePayload) {
    return request<Source>(`/api/sources/${id}`, {
      method: 'PATCH',
      body: JSON.stringify(payload),
    })
  },

  deleteSource(id: string) {
    return request<void>(`/api/sources/${id}`, {
      method: 'DELETE',
    })
  },

  assignAgent(id: string, assignedAgentId: string | null) {
    return request<Source>(`/api/sources/${id}/agent`, {
      method: 'PUT',
      body: JSON.stringify({ assigned_agent_id: assignedAgentId }),
    })
  },

  async listArticles(sourceId?: string) {
    const query = sourceId ? `?source_id=${encodeURIComponent(sourceId)}` : ''
    const payload = await request<ArticleListResponse>(`/api/articles${query}`)
    return payload.items
  },

  async listFavorites() {
    const payload = await request<ArticleListResponse>('/api/favorites')
    return payload.items
  },

  getArticle(id: string) {
    return request<ArticleDetail>(`/api/articles/${id}`)
  },

  setReadState(id: string, read: boolean) {
    return request<ArticleDetail>(`/api/articles/${id}/read`, {
      method: 'PUT',
      body: JSON.stringify({ read }),
    })
  },

  setReadStates(articleIds: string[], read: boolean) {
    return request<{ updated: number; read_at: string | null }>('/api/articles/read', {
      method: 'PUT',
      body: JSON.stringify({ article_ids: articleIds, read }),
    })
  },

  setFavorite(id: string, favorited: boolean) {
    return request<ArticleDetail>(`/api/articles/${id}/favorite`, {
      method: 'PUT',
      body: JSON.stringify({ favorited }),
    })
  },

  listBookmarks() {
    return this.listFavorites()
  },

  setBookmark(id: string, bookmarked: boolean) {
    return this.setFavorite(id, bookmarked)
  },
}
