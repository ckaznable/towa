## ADDED Requirements

### Requirement: Reader API exposes health, sources, articles, agents, favorites, and admin processing endpoints
The system SHALL provide HTTP endpoints for the web client to inspect service health, manage sources, list agents, read articles, manage favorites, and inspect processing state.

#### Scenario: Client reads reader data
- **WHEN** the web client requests reader-facing endpoints
- **THEN** the system SHALL return JSON contracts for sources, articles, agents, favorites, and processing overview data

#### Scenario: Client updates reader state
- **WHEN** the web client submits source changes, agent assignment changes, favorite changes, or retry requests
- **THEN** the system SHALL persist those changes and return updated API responses

### Requirement: Non-API routes fall back to the web application entrypoint
The system SHALL serve the built web application for non-API routes so that the SPA router can handle in-app navigation.

#### Scenario: Browser requests a nested front-end route
- **WHEN** the browser requests a non-API route such as a reader or settings path
- **THEN** the server SHALL return the web application entrypoint instead of a not-found response

### Requirement: Web UI supports the main reader workflows
The system SHALL support dashboard, article reading, favorites, and settings workflows through the shipped web client.

#### Scenario: User browses the main article stream
- **WHEN** the user opens the dashboard
- **THEN** the web client SHALL load sources and article stream data from the reader API

#### Scenario: User opens a saved items view
- **WHEN** the user switches to the favorites view
- **THEN** the web client SHALL request the favorites collection and render only favorited articles

#### Scenario: User manages feed sources in settings
- **WHEN** the user creates, edits, deletes, or reassigns a source in settings
- **THEN** the web client SHALL call the corresponding reader API endpoints and refresh the displayed source state
