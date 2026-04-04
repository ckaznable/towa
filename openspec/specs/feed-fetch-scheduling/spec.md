# feed-fetch-scheduling Specification

## Purpose
Define the contract for background feed fetching, conditional requests, freshness header handling, and fallback scheduling intervals.

## Requirements
### Requirement: Scheduler fetches due sources in the background
The system SHALL run a background scheduler that periodically checks for due sources and fetches only sources whose `next_fetch_at` is due or unset.

#### Scenario: Scheduler finds a due source
- **WHEN** a source is enabled and its `next_fetch_at` is missing or earlier than the current time
- **THEN** the scheduler SHALL attempt to fetch that source

#### Scenario: Scheduler skips a non-due source
- **WHEN** a source has a `next_fetch_at` later than the current time
- **THEN** the scheduler SHALL leave that source untouched for the current cycle

### Requirement: Scheduler uses conditional requests when possible
The system SHALL reuse stored `ETag` and `Last-Modified` values to make conditional feed requests.

#### Scenario: Source has cached validators
- **WHEN** a source has stored `ETag` or `Last-Modified` metadata
- **THEN** the scheduler SHALL include those validators in the next HTTP request

#### Scenario: Feed responds as not modified
- **WHEN** a feed responds with a not-modified result
- **THEN** the system SHALL avoid re-parsing article content
- **AND** the system SHALL still compute and persist the next fetch time

### Requirement: Scheduler respects freshness headers before fallback intervals
The system SHALL compute the next fetch time using response freshness metadata before applying fallback scheduling rules.

#### Scenario: Feed returns Cache-Control max-age
- **WHEN** a fetch response contains `Cache-Control: max-age` or `s-maxage`
- **THEN** the system SHALL schedule the next fetch using that header value

#### Scenario: Feed returns Expires without Cache-Control max-age
- **WHEN** a fetch response contains a future `Expires` header and no usable max-age value
- **THEN** the system SHALL schedule the next fetch using the expiry time

#### Scenario: Feed returns no usable freshness headers
- **WHEN** a fetch response contains no usable freshness headers
- **THEN** the system SHALL fall back to the internal scheduling interval rules

### Requirement: Scheduler applies bounded fallback fetch intervals
The system SHALL use a bounded fallback interval when freshness headers are absent or when fetches fail.

#### Scenario: Modified or first fetch without freshness headers
- **WHEN** a source is newly fetched or a fetch returns modified content without usable freshness headers
- **THEN** the system SHALL schedule the next fetch using the minimum fallback interval

#### Scenario: Repeated not-modified fetch without freshness headers
- **WHEN** a source repeatedly returns not modified and no usable freshness headers are available
- **THEN** the system SHALL increase the next fetch interval from the previous interval
- **AND** the interval SHALL remain within the configured minimum and maximum bounds

#### Scenario: Fetch fails
- **WHEN** a source fetch fails
- **THEN** the system SHALL reschedule the source for a later retry
- **AND** the retry SHALL use the minimum fallback interval
