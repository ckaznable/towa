## ADDED Requirements

### Requirement: System can manage RSS and Atom feed sources
The system SHALL allow clients to create, list, update, delete, enable, disable, and inspect feed sources for both RSS and Atom formats.

#### Scenario: Create a valid feed source
- **WHEN** a client submits a source with a valid absolute HTTP or HTTPS feed URL
- **THEN** the system SHALL validate the feed
- **AND** the system SHALL persist the source with feed metadata and enabled state

#### Scenario: Update an existing feed source
- **WHEN** a client updates the title, URL, or enabled state of an existing source
- **THEN** the system SHALL persist the updated source fields
- **AND** subsequent source detail and list responses SHALL reflect the updated values

#### Scenario: Delete a feed source
- **WHEN** a client deletes an existing source
- **THEN** the system SHALL remove the source
- **AND** subsequent lookups for that source SHALL fail as not found

### Requirement: System can assign an LLM agent to a feed source
The system SHALL allow each source to carry an optional assigned agent identifier used by downstream article processing.

#### Scenario: Assign an existing agent to a source
- **WHEN** a client assigns a configured agent identifier to a source
- **THEN** the system SHALL persist the assigned agent on that source

#### Scenario: Clear an assigned agent from a source
- **WHEN** a client clears the assigned agent on a source
- **THEN** the system SHALL persist the source with no assigned agent

### Requirement: Feed source API exposes validation and scheduling metadata
The system SHALL expose source validation state and fetch scheduling metadata through source list and detail responses.

#### Scenario: Inspect source metadata
- **WHEN** a client requests the source list or a specific source
- **THEN** the system SHALL return validation status
- **AND** the system SHALL return last and next fetch timestamps when available
