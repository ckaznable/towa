## ADDED Requirements

### Requirement: Newly fetched articles can enter agent-based post-processing
The system SHALL place newly fetched articles into LLM post-processing when their source has an assigned agent.

#### Scenario: Source has an assigned agent
- **WHEN** the scheduler stores a new article for a source with an assigned agent
- **THEN** the system SHALL place the article into a pending processing state

#### Scenario: Source has no assigned agent
- **WHEN** the scheduler stores a new article for a source without an assigned agent
- **THEN** the system SHALL mark the article as complete without LLM output generation

### Requirement: LLM worker submits pending work to Gemini batch processing
The system SHALL group pending jobs by assigned agent and submit them to Gemini batch processing in bounded batches.

#### Scenario: Pending jobs exist for the same agent
- **WHEN** the worker finds pending jobs for a configured agent
- **THEN** the worker SHALL submit those jobs as a Gemini batch
- **AND** the system SHALL record a batch identifier for later polling

### Requirement: LLM worker tracks processing states and outputs
The system SHALL expose article processing state and write resulting summaries or errors back to persisted article processing records.

#### Scenario: Batch completes successfully
- **WHEN** Gemini returns a successful output for an article
- **THEN** the system SHALL mark that article as done
- **AND** the system SHALL persist the generated summary

#### Scenario: Batch returns an article-level error
- **WHEN** Gemini returns an error for a specific article in a batch
- **THEN** the system SHALL persist the error for that article
- **AND** the system SHALL transition the article according to retry policy

### Requirement: Failed processing can be retried within configured limits
The system SHALL retry failed processing jobs up to the configured retry limit and SHALL expose admin retry controls.

#### Scenario: Processing fails before retry limit is reached
- **WHEN** a processing attempt fails and the retry limit has not been exhausted
- **THEN** the system SHALL return the article to pending state for retry

#### Scenario: Processing fails after retry limit is reached
- **WHEN** a processing attempt fails and the retry limit is exhausted
- **THEN** the system SHALL mark the article as failed
- **AND** the last error SHALL remain queryable

#### Scenario: Operator retries a failed article or batch
- **WHEN** a client calls the admin retry API for an article or a batch
- **THEN** the system SHALL move eligible matching jobs back to pending state
