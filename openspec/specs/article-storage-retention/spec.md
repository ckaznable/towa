# article-storage-retention Specification

## Purpose
Define the contract for persisted article storage, per-source deduplication, and retention cleanup behavior.

## Requirements
### Requirement: System persists fetched articles in SQLite
The system SHALL persist fetched articles, their source association, fetch timestamps, URLs, summaries, and processing metadata in SQLite.

#### Scenario: Store a newly fetched article
- **WHEN** the scheduler parses a previously unseen article from a feed
- **THEN** the system SHALL persist that article and its source association in SQLite

#### Scenario: Read stored article through API
- **WHEN** a client requests the article list or article detail
- **THEN** the system SHALL return data sourced from persisted article records

### Requirement: System deduplicates repeated feed items per source
The system SHALL avoid creating duplicate articles for the same source item across repeated fetches.

#### Scenario: Scheduler sees the same feed item again
- **WHEN** the scheduler fetches a source containing an item already represented by the same dedupe key for that source
- **THEN** the system SHALL update the existing article record instead of inserting a duplicate

### Requirement: Non-favorited articles follow the normal retention window
The system SHALL remove expired non-favorited articles after the standard retention window during background cleanup.

#### Scenario: Cleanup runs on expired non-favorited articles
- **WHEN** an article is older than the normal retention window and is not favorited
- **THEN** the background cleanup SHALL delete that article

#### Scenario: Cleanup runs on recent non-favorited articles
- **WHEN** an article is still within the normal retention window and is not favorited
- **THEN** the background cleanup SHALL retain that article

### Requirement: Favorited articles remain exempt from normal cleanup
The system SHALL preserve favorited articles during normal retention cleanup.

#### Scenario: Cleanup runs on expired favorited articles
- **WHEN** an article is older than the normal retention window and is favorited
- **THEN** the background cleanup SHALL retain that article
