# Web3 Agent Capability Infrastructure

**The Open Intelligence and Capability Layer for Web3 Agents**

**Project Whitepaper · Version 1.0 · November 2026**

> **Repository context.** This repository currently implements **Fluxion**, the `swap.quote`
> engine behind this vision: pool discovery, exact-in routing, split allocation, and local
> snapshot freshness on Ethereum mainnet. The Local Runtime, wallet/execution capabilities,
> Skills, and hosted Remote Intelligence described here are the broader platform direction.
> See also [README](../README.md), [architecture](ARCHITECTURE.md), and
> [local state](LOCAL-STATE.md).

## Abstract

Artificial Intelligence Agents are rapidly evolving from conversational assistants into autonomous software agents capable of planning, reasoning, writing code, interacting with APIs, and executing complex workflows.

However, interacting with Web3 remains difficult.

Web3 capabilities are fragmented across blockchains, wallets, protocols, data providers, APIs, and developer tools. Each protocol has its own interfaces, transaction requirements, data structures, and security assumptions.

An AI Agent may understand what a user wants to accomplish, but it does not necessarily possess the infrastructure required to discover opportunities, understand protocols, evaluate risks, construct transactions, and interact with wallets safely.

**Web3 Agent Capability Infrastructure aims to solve this problem.**

We are building an open, extensible capability infrastructure that allows existing AI Agents to discover, understand, analyze, and interact with Web3 through standardized interfaces.

Rather than building another AI Agent, we provide the capabilities that existing Agents need.

The infrastructure consists of three core components:

- **CLI + Skills:** A universal interface through which AI Agents access Web3 capabilities.
- **Local Runtime:** A local execution environment responsible for wallet connectivity, user authorization, transaction construction, signing, and execution.
- **Remote Intelligence:** A cloud-based infrastructure responsible for protocol discovery, blockchain data aggregation, opportunity intelligence, risk analysis, and capability coordination.

Our long-term vision is to make Web3 capabilities accessible to any AI Agent, regardless of its framework, model provider, wallet, or development environment.

## 1. The Problem

### 1.1 AI Agents are becoming capable, but Web3 remains fragmented

Modern AI Agents can already:

- Understand natural language instructions.
- Plan multi-step tasks.
- Search for information.
- Call external tools and APIs.
- Write and execute code.
- Coordinate complex workflows.

Yet Web3 introduces several additional challenges.

**Fragmented ecosystems**

Different blockchains and protocols expose different interfaces, data formats, and interaction mechanisms.

**Complex protocol discovery**

Finding a suitable DeFi opportunity requires searching across protocols, chains, markets, and assets.

**Inconsistent financial data**

APY, TVL, liquidity, rewards, historical performance, and risk information are distributed across different sources and are not always directly comparable.

**Difficult execution workflows**

An operation may require approvals, contract interactions, transaction simulation, wallet signatures, and post-execution verification.

**Security and trust**

An Agent must not be given unrestricted access to private keys or unlimited authority over user assets.

The result is that developers repeatedly rebuild similar infrastructure for different Agents and applications.

### 1.2 The missing infrastructure

We believe the missing component is not another AI Agent.

It is a standardized Web3 capability layer that allows any Agent to interact with Web3 in a consistent, secure, and composable way.

The Agent should decide what to do.

The infrastructure should provide the capabilities required to do it.

The user should retain control over financial authorization.

## 2. Our Vision

**The Open Intelligence and Capability Layer for Web3 Agents**

Our vision is to make Web3 as accessible to AI Agents as the internet is to modern applications.

An Agent should be able to understand a user request such as:

- Find the best ETH staking opportunities.
- Compare USDC lending yields on Base.
- Find suitable liquidity pools.
- Analyze the risks of a DeFi protocol.
- Deposit assets into a selected market.
- Monitor an existing position.
- Withdraw assets when certain conditions are met.

The Agent should not need to understand every protocol's internal implementation.

Instead, it accesses standardized capabilities provided by our infrastructure.

We are not building a closed Agent ecosystem.

We are building an open infrastructure that existing Agents can use.

## 3. Product Architecture

The infrastructure consists of four logical layers.

### 3.1 Agent Layer

Users continue using their preferred AI Agents.

Supported environments may include:

- OpenAI Codex
- Claude Code
- OpenCode
- Other compatible AI Agent frameworks

We do not require users to migrate to our own Agent or model.

Our infrastructure remains independent of the Agent's underlying model provider.

### 3.2 Developer Interface: CLI + Skills

CLI and Skills form the primary developer interface.

**CLI**

The CLI provides deterministic, executable Web3 capabilities.

It is responsible for:

- Wallet operations.
- Blockchain queries.
- Protocol discovery.
- Market data retrieval.
- Transaction construction.
- Transaction simulation.
- Transaction execution.
- Transaction verification.

CLI commands return structured outputs, primarily JSON, so Agents can reliably consume and combine results.

**Skills**

Skills provide the knowledge and instructions required to use our capabilities.

They describe:

- Available capabilities.
- Command usage.
- Workflow composition.
- Required parameters.
- Expected outputs.
- Security requirements.
- Error handling.
- Transaction confirmation procedures.

Skills guide the Agent but are not themselves a security boundary.

All critical authorization and transaction restrictions must be enforced by the Runtime.

### 3.3 Local Runtime

Local Runtime operates on the user's machine.

Its primary responsibility is to protect user-controlled operations.

Core capabilities include:

**Wallet Management**

- Wallet connection.
- Account discovery.
- Wallet session management.
- Chain selection.
- Account status.

**Asset Management**

- Token balances.
- Portfolio information.
- User positions.
- Allowance inspection.

**Transaction Management**

- Transaction construction.
- Parameter validation.
- Transaction simulation.
- User confirmation.
- Wallet signing.
- Transaction broadcasting.
- Execution verification.

**Security**

- Local authorization policies.
- Spending limits.
- Chain restrictions.
- Contract allowlists.
- Transaction inspection.
- Audit logs.

Private keys and wallet secrets should remain under user-controlled wallet security boundaries.

The infrastructure must never require users to upload private keys to our Remote service.

### 3.4 Remote Intelligence

Remote Intelligence is the central cloud infrastructure of the project.

It provides capabilities that are inefficient, expensive, or difficult to reproduce independently on every user's machine.

Its responsibilities include:

**Protocol Intelligence**

- Protocol discovery.
- Protocol metadata.
- Contract information.
- Supported chains.
- Supported assets.
- Supported actions.

**Market Intelligence**

- APY and yield information.
- TVL.
- Liquidity.
- Historical performance.
- Market conditions.
- Reward information.

**Opportunity Discovery**

- Staking opportunities.
- Lending markets.
- Borrowing markets.
- Liquidity provision opportunities.
- Restaking opportunities.
- Cross-chain opportunities.

**Risk Intelligence**

- Protocol risk indicators.
- Liquidity risks.
- Lock-up conditions.
- Withdrawal restrictions.
- Smart contract information.
- Data quality and freshness.
- Historical risk signals.

**Execution Knowledge**

- Protocol interaction requirements.
- Required parameters.
- Supported operations.
- Transaction templates.
- Simulation requirements.
- Verification rules.

Remote Intelligence provides structured information and capabilities.

It does not independently decide what users should invest in, nor does it authorize financial transactions on their behalf.

## 4. Local and Remote Capability Architecture

One of the fundamental design principles is the separation of capabilities by execution location.

### 4.1 Local Capabilities

Local capabilities are executed in the user's environment.

Examples:

| Capability | Responsibility |
| --- | --- |
| wallet.connect | Connect a wallet |
| wallet.status | Inspect wallet status |
| wallet.accounts | Retrieve accounts |
| wallet.balance | Query account balances |
| transaction.build | Construct a transaction |
| transaction.simulate | Simulate execution |
| transaction.approve | Request user authorization |
| transaction.sign | Sign through the connected wallet |
| transaction.execute | Broadcast a transaction |
| transaction.verify | Verify execution |

Local capabilities are especially important when user authorization, wallet sessions, or private information are involved.

### 4.2 Remote Capabilities

Remote capabilities are executed by our cloud infrastructure.

Examples:

| Capability | Responsibility |
| --- | --- |
| protocol.discover | Discover protocols |
| protocol.get | Retrieve protocol information |
| market.search | Search markets |
| market.get | Retrieve market information |
| yield.discover | Discover yield opportunities |
| yield.history | Retrieve historical yields |
| risk.analyze | Retrieve risk analysis and evidence |
| position.analyze | Analyze position information |
| capability.discover | Discover available capabilities |

Remote capabilities provide aggregated and normalized information.

They should support multiple underlying data providers instead of depending on a single source.

### 4.3 Hybrid Capabilities

Some capabilities naturally involve both Local and Remote execution.

For example, transaction construction may require:

1. Remote protocol metadata.
2. Remote execution requirements.
3. Local wallet and account information.
4. Local transaction construction.
5. Local simulation and validation.
6. User authorization and signing.

Therefore, Local and Remote should be treated as complementary execution environments within one capability system.

## 5. Capability-First Architecture

### 5.1 Organizing around user actions

Traditional Web3 infrastructure is commonly organized around protocols.

For example:

Aave:

- Supply
- Withdraw
- Borrow
- Repay

Lido:

- Stake
- Unstake

Morpho:

- Supply
- Withdraw
- Borrow

However, users and AI Agents usually express their intentions as actions rather than protocol names.

Examples:

- Stake ETH.
- Supply USDC.
- Borrow USDC.
- Provide liquidity.
- Withdraw assets.

Our infrastructure adopts a capability-first architecture.

Instead of requiring Agents to understand each protocol independently, we expose standardized capabilities.

Example:

```text
staking.discover
staking.analyze
staking.quote
staking.build
staking.execute
staking.verify

lending.discover
lending.analyze
lending.supply
lending.withdraw

swap.quote
swap.build
swap.execute

liquidity.discover
liquidity.analyze
liquidity.provide
liquidity.withdraw
```

These are conceptual capability identifiers. The exact interface and command specification will be defined through the implementation standard.

### 5.2 Protocol Adapters

Each protocol integrates through an adapter.

An adapter describes:

- Supported chains.
- Supported assets.
- Supported actions.
- Required parameters.
- Data mappings.
- Transaction construction rules.
- Simulation requirements.
- Verification logic.

For example:

```text
Capability: staking.stake

Protocol Adapters:
  ├── Lido
  ├── Rocket Pool
  └── Other Staking Protocols
```

When a new protocol is integrated, its adapter maps protocol-specific operations to standardized capabilities.

This allows the infrastructure to expand without requiring every Agent to implement every protocol independently.

### 5.3 Data Providers

Protocol Adapters and Data Providers are separate components.

A Data Provider supplies information.

A Protocol Adapter understands how to interact with a protocol.

Data providers may include:

- Blockchain RPC endpoints.
- Official protocol APIs.
- Subgraphs.
- Indexing services.
- Historical data providers.
- Independent analytics sources.

Multiple providers can support the same protocol.

This separation improves flexibility, data reliability, and provider independence.

## 6. Remote Intelligence: The Strategic Core

Remote Intelligence is expected to become the most strategically valuable component of the project.

The goal is not to build another generic blockchain API service.

The goal is to build a structured Web3 knowledge and intelligence infrastructure that AI Agents can consume directly.

### 6.1 Web3 Knowledge Graph

We will progressively build a structured representation of relationships across the Web3 ecosystem.

Core entities include:

- Chains.
- Protocols.
- Contracts.
- Assets.
- Markets.
- Validators.
- Pools.
- Capabilities.
- Transactions.
- Risk indicators.

Relationships include:

- Protocol supports Chain.
- Market accepts Asset.
- Protocol implements Capability.
- Contract executes Action.
- Position belongs to Market.
- Opportunity uses Protocol.

This knowledge graph enables Agents to discover relationships rather than repeatedly searching unstructured information.

### 6.2 Unified Data Model

Different protocols calculate and expose information differently.

Our infrastructure should normalize important concepts.

For example, yield information should include:

```text
{
  "asset": "ETH",
  "action": "stake",
  "protocol": "Example Protocol",
  "chain": "ethereum",
  "yield": {
    "value": 4.2,
    "unit": "percent",
    "type": "estimated_apy",
    "components": []
  },
  "liquidity": {},
  "lockup": {},
  "risks": [],
  "source": [],
  "updated_at": ""
}
```

The model must preserve important distinctions.

For example:

- Historical APY is not current APY.
- Estimated APY is not guaranteed APY.
- Native staking rewards are not necessarily equivalent to DeFi incentives.
- TVL is not the same as available liquidity.
- A high APY does not automatically indicate a good opportunity.

Data normalization must not remove the original meaning or provenance of the source information.

### 6.3 Opportunity Intelligence

Opportunity Discovery should return more than a list of APY numbers.

An opportunity should contain:

- Protocol information.
- Supported chain.
- Supported asset.
- Available action.
- Yield information.
- Yield calculation methodology.
- Historical performance.
- Liquidity information.
- Lock-up and withdrawal conditions.
- Risk indicators.
- Source references.
- Data timestamps.
- Execution capability information.

The Agent can then compare opportunities according to the user's preferences and constraints.

### 6.4 Risk Intelligence

Risk information should be structured and evidence-based.

Potential indicators include:

- Smart contract risk.
- Protocol maturity.
- Liquidity risk.
- Withdrawal limitations.
- Oracle dependencies.
- Centralization dependencies.
- Historical incidents.
- Asset depeg exposure.
- Reward sustainability.
- Data confidence.

Our system should distinguish between observed facts, derived metrics, and analytical assessments.

We do not promise that any protocol is safe or that any yield is guaranteed.

### 6.5 Continuous Data Infrastructure

Remote should progressively support:

- Data indexing.
- Data normalization.
- Historical snapshots.
- Caching.
- Data freshness monitoring.
- Provider health monitoring.
- Anomaly detection.
- Protocol metadata updates.

This enables Remote to serve many Agents without every user independently repeating the same data retrieval and analysis operations.

## 7. Execution and Security Model

Security is a foundational requirement, not an optional feature.

### 7.1 User-Controlled Assets

Users retain control of their wallets and assets.

Our Remote infrastructure should not require custody of user funds.

Private keys should not be uploaded to our cloud infrastructure.

Signing should occur through user-controlled wallet mechanisms or explicitly configured local signing providers.

### 7.2 Transaction Lifecycle

Every financial operation should follow a controlled lifecycle:

```text
User Intent
    |
    v
Capability Discovery
    |
    v
Opportunity Analysis
    |
    v
Transaction Construction
    |
    v
Transaction Validation
    |
    v
Transaction Simulation
    |
    v
Security Policy Check
    |
    v
User Approval
    |
    v
Wallet Signing
    |
    v
Transaction Broadcasting
    |
    v
Execution Verification
```

### 7.3 Explicit User Authorization

An Agent may independently:

- Discover opportunities.
- Compare markets.
- Analyze information.
- Recommend an operation.
- Construct a transaction proposal.
- Run permitted simulations.

However, signing and financial authorization must follow the user's configured permissions.

The system should support explicit confirmation before sensitive transactions.

Authorization must be bound to the exact transaction parameters.

Relevant parameters include:

- Chain ID.
- Destination contract.
- Transaction value.
- Calldata.
- Token address.
- Token amount.
- Allowance amount.
- Gas parameters.
- Relevant protocol operation.

If transaction content changes after authorization, the previous confirmation must no longer be considered valid.

Token approvals must be treated as financial operations with their own risks.

### 7.4 Policy Engine

The Local Runtime should support configurable policies, such as:

- Allowed chains.
- Allowed protocols.
- Allowed contracts.
- Maximum transaction amount.
- Token spending limits.
- Allowed operations.
- Session expiration.
- Human confirmation requirements.

The Agent must not be able to bypass these restrictions simply by changing its Skills instructions.

### 7.5 Simulation Is Not a Guarantee

Simulation provides useful execution evidence but cannot guarantee future transaction success or protocol safety.

Blockchain state may change between simulation and execution.

Market conditions, liquidity, gas costs, MEV, and contract behavior may also change.

The infrastructure must communicate these limitations clearly.

## 8. Open and Extensible Ecosystem

We believe the long-term infrastructure should be open and extensible.

### 8.1 Agent Agnostic

The infrastructure should not depend on one AI Agent or model provider.

Any compatible Agent should be able to use the same capabilities.

### 8.2 Provider Agnostic

Remote should support multiple data providers.

Users and developers should eventually be able to configure alternative providers or operate their own Remote infrastructure.

Our hosted service will provide the default experience.

### 8.3 Protocol Agnostic

The architecture should not be permanently tied to one protocol or blockchain.

Initial implementation may focus on EVM, but the capability model should support future integration with other ecosystems.

### 8.4 Community Extensions

We aim to allow third-party developers to contribute:

- Protocol Adapters.
- Data Providers.
- Capability Definitions.
- Skills.
- Chain Integrations.
- Analysis Modules.

The ecosystem should provide a clear mechanism for validating, versioning, and distributing extensions.

Security-sensitive adapters should require additional verification and review.

## 9. CLI and Skills Experience

The CLI is the primary interface for the first version.

Example installation:

```text
npm install -g agent-web3

agent-web3 init

agent-web3 wallet connect
```

Example commands:

```text
# Wallet
agent-web3 wallet status
agent-web3 wallet accounts
agent-web3 wallet balance

# Discovery
agent-web3 defi discover \
  --action stake \
  --asset ETH

# Analysis
agent-web3 defi analyze <opportunity-id>

# Transaction
agent-web3 tx build <opportunity-id>
agent-web3 tx simulate <transaction-id>
agent-web3 tx execute <transaction-id>
agent-web3 tx verify <transaction-id>
```

Commands are illustrative and may evolve during implementation.

The CLI should provide:

- Consistent command structure.
- Structured JSON output.
- Clear error codes.
- Stable identifiers.
- Traceable execution.
- Capability version information.

Skills will teach compatible Agents how to compose these commands into meaningful workflows.

The initial Skills may include:

```text
skills/
  wallet/
  chain-data/
  defi-discovery/
  defi-analysis/
  defi-execution/
  transaction-safety/
```

MCP and SDK integrations may be added later, but they are not required to establish the initial product.

## 10. Business Model

The project should combine open infrastructure with sustainable hosted services.

### 10.1 Open Source Core

The core CLI, Local Runtime interfaces, capability specifications, and development tools should be open source.

This allows developers to inspect, extend, and contribute to the infrastructure.

### 10.2 Hosted Remote Services

Our hosted Remote service may offer:

- Managed data indexing.
- Advanced opportunity discovery.
- Historical datasets.
- Higher request limits.
- Advanced risk intelligence.
- Premium data sources.
- Monitoring and notifications.
- Enterprise service levels.

A free tier can reduce the barrier to adoption.

### 10.3 Ecosystem Integrations

Protocols and Web3 projects may sponsor integrations or provide funding for ecosystem access.

However, sponsored integrations must be clearly disclosed.

Commercial sponsorship must not silently influence rankings or risk assessments.

### 10.4 Developer and Enterprise Services

Potential customers include:

- AI Agent developers.
- Web3 application developers.
- Wallet providers.
- DeFi protocols.
- Agent infrastructure platforms.
- Institutional Web3 teams.

Potential revenue sources include:

- Hosted API usage.
- Premium intelligence services.
- Enterprise subscriptions.
- Dedicated indexing infrastructure.
- Commercial integrations.
- Managed Remote deployments.

The long-term goal is to create a sustainable infrastructure business rather than relying exclusively on token incentives.

## 11. Roadmap

The project should be developed incrementally.

### Phase 1 — Foundation

**Objective: Establish the basic Agent-to-Web3 capability workflow.**

Scope:

- CLI foundation.
- Skills installation.
- Local Runtime.
- Wallet connectivity.
- EVM support.
- Base integration.
- Remote protocol discovery.
- Standardized JSON output.

The first version should prioritize read-only capabilities and reliable discovery.

### Phase 2 — Remote Intelligence

**Objective: Build differentiated data and discovery capabilities.**

Scope:

- Protocol registry.
- Asset and market registry.
- APY normalization.
- TVL and liquidity information.
- Historical data.
- Opportunity discovery.
- Data source tracking.
- Data freshness monitoring.

### Phase 3 — DeFi Execution

**Objective: Complete the discovery-to-execution workflow.**

Scope:

- Protocol Adapters.
- Transaction construction.
- Transaction simulation.
- Security policies.
- User authorization.
- Wallet signing.
- Transaction verification.

The first supported DeFi workflow should be deliberately narrow.

### Phase 4 — Open Ecosystem

**Objective: Allow external developers to extend the infrastructure.**

Scope:

- Capability Registry.
- Adapter development specification.
- Third-party Provider support.
- Extension validation.
- Additional chains.
- Additional protocols.
- Developer documentation.

### Phase 5 — Infrastructure Network

**Objective: Become a widely used capability layer for Web3 Agents.**

Scope:

- Advanced Remote Intelligence.
- Distributed provider ecosystem.
- Self-hosted Remote.
- Enterprise infrastructure.
- Advanced monitoring.
- Broader Web3 coverage.

## 12. Initial Technical Scope

To avoid building too much before validating the product, the initial implementation should focus on one complete use case.

Proposed starting environment:

| Component | Initial choice |
| --- | --- |
| Interface | CLI + Skills |
| Runtime | Local-first |
| Blockchain | EVM |
| Initial chain | Base |
| Wallet | WalletConnect-compatible wallets |
| Initial asset | USDC or ETH |
| Remote | Hosted Remote service |
| Initial data | Protocol and market discovery |
| Execution | Limited protocol Adapter |
| Output | Structured JSON |

A possible first workflow is:

**Discover→Analyze→Build→Simulate→Approve→Execute→Verify**

The initial protocol and operation should be selected based on integration complexity, data availability, and actual user demand.

We should not attempt to support every chain, protocol, and DeFi action in the first release.

## 13. Design Principles

The project follows ten principles.

1. **Agent Agnostic:** Do not require users to change their preferred Agent.
2. **Local First:** Sensitive wallet operations remain under user control.
3. **Remote Intelligence:** Centralize expensive, reusable data and intelligence capabilities.
4. **Capability First:** Organize functionality around actions rather than individual protocols.
5. **Protocol Agnostic:** Avoid permanent dependency on one protocol or ecosystem.
6. **Provider Agnostic:** Support multiple data sources and replaceable providers.
7. **Open and Extensible:** Encourage community-built adapters and capabilities.
8. **Transparent:** Provide evidence, sources, timestamps, and limitations.
9. **Secure by Design:** Enforce transaction permissions outside the Agent's instructions.
10. **Composable:** Allow Agents to combine small capabilities into complex workflows.

## 14. Long-Term Vision

We believe the next generation of Web3 applications will increasingly be operated by AI Agents.

Users will not necessarily open multiple DApps, compare dozens of dashboards, manually construct transactions, or learn every protocol's interface.

Instead, they will describe what they want to accomplish.

Their Agents will discover available opportunities, understand relevant constraints, analyze alternatives, and coordinate the required operations.

For this future to work, Agents need reliable infrastructure.

They need access to Web3 knowledge.

They need standardized capabilities.

They need safe execution environments.

They need verifiable results.

**We are building that infrastructure.**

Not another AI Agent.

Not another wallet.

Not another DeFi dashboard.

We are building the open capability layer that connects AI Agents to the Web3 ecosystem.

## Project Statement

**Web3 Agent Capability Infrastructure**

*Making Web3 discoverable, understandable, and executable for every AI Agent.*

The Agent is the brain.

Our infrastructure provides the capabilities.

The user remains in control.