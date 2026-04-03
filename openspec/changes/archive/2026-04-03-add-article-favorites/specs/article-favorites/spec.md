## ADDED Requirements

### Requirement: User can favorite and unfavorite articles
The system SHALL allow a client to mark an existing article as favorited and to remove that favorite state later without changing the article's content, source association, or LLM processing result.

#### Scenario: Favorite an article
- **WHEN** a client requests that an existing article be favorited
- **THEN** the system SHALL persist the article as favorited
- **AND** subsequent article detail responses SHALL report the article as favorited

#### Scenario: Unfavorite an article
- **WHEN** a client requests that a favorited article be unfavorited
- **THEN** the system SHALL persist the article as not favorited
- **AND** subsequent article detail responses SHALL report the article as not favorited

### Requirement: System can return a favorites collection
The system SHALL provide a dedicated way to retrieve favorited articles as a collection distinct from the general article stream.

#### Scenario: List favorited articles
- **WHEN** a client requests the favorites collection
- **THEN** the system SHALL return only articles whose favorite state is enabled
- **AND** each returned article SHALL preserve its source metadata and LLM processing status

### Requirement: Favorited articles are exempt from normal retention cleanup
The system SHALL exclude favorited articles from the normal retention cleanup that removes non-favorited articles after the standard retention window.

#### Scenario: Cleanup runs with favorited and non-favorited old articles
- **WHEN** retention cleanup runs on articles older than the normal retention window
- **THEN** non-favorited articles SHALL be eligible for deletion
- **AND** favorited articles SHALL remain stored

#### Scenario: Unfavorited article returns to normal retention rules
- **WHEN** a previously favorited article is later unfavorited
- **THEN** the article SHALL once again be governed by the standard retention policy

### Requirement: Favorite operations preserve article processing state
The system SHALL treat favorite state as an independent reading preference and MUST NOT reset or modify article processing state during favorite operations.

#### Scenario: Favorite an article with completed LLM output
- **WHEN** a client favorites an article that already has LLM output
- **THEN** the system SHALL preserve the article's existing `llm_status`, `llm_summary`, and `llm_error` values

#### Scenario: Favorite an article that is still pending processing
- **WHEN** a client favorites an article whose LLM processing is pending or in progress
- **THEN** the system SHALL preserve the current processing state
- **AND** the article SHALL remain eligible for the existing processing workflow
