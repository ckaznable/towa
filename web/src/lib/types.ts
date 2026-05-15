export type FeedKind = 'rss' | 'atom'
export type ProcessingStatus = 'pending' | 'processing' | 'done' | 'failed'

export interface AgentSummary {
  id: string
  label: string
  provider: string
  model: string
  batch_enabled: boolean
}

export interface Source {
  id: string
  title: string
  feed_url: string
  feed_kind: FeedKind
  enabled: boolean
  assigned_agent_id: string | null
  validation_status: string
  last_fetch_at: string | null
  next_fetch_at: string | null
  created_at: string
  updated_at: string
}

export interface ArticleListItem {
  id: string
  source_id: string
  source_title: string
  title: string
  llm_title: string | null
  summary: string
  content: string
  url: string
  published_at: string | null
  fetched_at: string
  available_at: string
  read: boolean
  read_at: string | null
  favorited: boolean
  bookmarked: boolean
  llm_status: ProcessingStatus
}

export interface ArticleDetail extends ArticleListItem {
  llm_summary: string | null
  llm_error: string | null
}

export interface SourceListResponse {
  items: Source[]
}

export interface AgentListResponse {
  items: AgentSummary[]
}

export interface ArticleListResponse {
  items: ArticleListItem[]
  total: number
  limit: number
  offset: number
  has_more: boolean
}

export interface ArticleListParams {
  sourceId?: string | null
  favorited?: boolean
  bookmarked?: boolean
  read?: boolean
  llmStatus?: ProcessingStatus
  limit?: number
  offset?: number
}

export interface BulkReadStateResponse {
  updated: number
  read_at: string | null
}

export interface SourceUnreadCount {
  source_id: string
  unread: number
}

export interface ArticleUnreadCountsResponse {
  items: SourceUnreadCount[]
  total_unread: number
}

export interface CreateSourcePayload {
  title?: string
  feed_url: string
  enabled?: boolean
  assigned_agent_id?: string | null
}

export interface UpdateSourcePayload {
  title?: string
  feed_url?: string
  enabled?: boolean
}
