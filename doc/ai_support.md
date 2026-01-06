# AI Support in atomCAD

This document explores how AI can be integrated into atomCAD to improve the design process.
As the extent to which AI can be useful and accurate in this context is unknown, it
would be beneficial to find an initial solution that we can use to validate this idea
without huge development effort.

## Current trends in AI agent development

Recently coding agents like Claude Code have been gaining popularity in the field of software development. Increasingly Claude Code is also used for non-coding tasks as it can generate custom code to perform tasks. Recently Anthropic released  the Claude Agent SDK which enables Claude Code being integrated into applications.

The Claude Agent SDK has Python and Typescript APIs.

Question: how can we integrate the Claude Agent SDK into atomCAD?

## Node network textual projection vs. using a maintream language and its ecosystem.

The ideal way to provide AI assitance into atomCAD would be to create a textual representation of the node network, and use Claude Code to generate and modify node networks based on the user's instructions.

This approach has two main disadvantages though:

- AI models are not proficient with understanding custom languages. This can be mitigated to some extent by making the node network textual representation as close to a known programming language as possible. Such a language can be Elm.
- Developing the node network and its textual projection to a sufficient level requires significant effort.

Another way to add AI support to atomCAD is to just use a mainstraim language and its ecosystem as executable programs that generate cystolecule outputs. In this case we would just provide a low-level API for these programs to interact with atomCAD. This low level API would be just a thin layer upon the geo-tree and crystolecule modules. So basically these ai-created programs would be able to create geometry trees directly (without nodes) and calling the atom fill fnction to create crystolecules.

Advantages: 
- AI models are proficient with understanding mainstream languages.
- No need to develop a custom language

Disadvantage:
- The node network concept is a self-contained representation of a crystolecule that is not dpeendent on external programming languages and is designed to have graphical representation. IT is also integrated with the atomCAD editor. This means that the AI-generated programs cannot be manually edited in the atomCAD editor.

Dispite the disadvantage I would still prefer to go with a mainstream language and low-level API
as a start. This will allow us to quickly asses the AI model's intuition and capbility to generate and modify crystal geometries.

## Feedback loop

To create a good agentic coding expericence we need to give feedback to the model.
The following feedback needs to be provided:
- Compilations errors
- Runtime errors
- Runtime logs
- Textual representation of (resulting) geo-trees.
- Textual representation of (resulting) polygon representation of geo-trees.
- Answers to other queries on (resulting) geo-trees.
- Textual representation of (resulting) atomic structures. (e.g. .mol format)
- Rendered image of (resulting) atomic structures or geometries.

## Recommended Approach: TypeScript + Claude Agent SDK

TypeScript is the recommended language for AI integration for the following reasons:

1. **First-class SDK support** - Anthropic provides a native TypeScript SDK for the Claude Agent SDK
2. **AI proficiency** - Claude models are highly proficient with TypeScript, resulting in more reliable code generation
3. **Type safety** - TypeScript's type system helps catch errors early, improving the feedback loop
4. **Ecosystem** - Access to npm packages for math, geometry, and scientific computing if needed

### Architecture

```mermaid
flowchart TB
    subgraph Flutter["Flutter Frontend"]
        UI[atomCAD UI]
        Chat[AI Chat Panel]
    end
    
    subgraph Node["Node.js Subprocess"]
        SDK[Claude Agent SDK]
        Gen[Generated TS Code]
        API[CrystoleculeAPI Client]
    end
    
    subgraph Rust["Rust Backend"]
        IPC[IPC Server - JSON-RPC]
        GeoTree[geo_tree module]
        Crysto[crystolecule module]
        Render[renderer module]
    end
    
    Chat -->|"user prompt"| SDK
    SDK -->|"generates & executes"| Gen
    Gen -->|"calls"| API
    API -->|"JSON-RPC over stdio/TCP"| IPC
    IPC --> GeoTree
    IPC --> Crysto
    IPC --> Render
    Render -->|"screenshot feedback"| SDK
    Crysto -->|".mol output"| SDK
    IPC -->|"errors/results"| API
```

### Implementation Components

1. **Rust IPC Server** - JSON-RPC endpoint exposing geo-tree and crystolecule operations
2. **TypeScript API Client** - Thin wrapper calling the Rust backend
3. **Agent Loop** - Claude Agent SDK orchestrating code generation and execution
4. **Process Management** - Flutter spawns Bun subprocess for AI sessions

### Distribution: Bun and the Claude Agent SDK

**Key discovery:** Anthropic acquired Bun (the JavaScript/TypeScript runtime) in December 2025. This fundamentally changes the distribution story.

#### How the SDK is Distributed

- **Python SDK:** `pip install claude-agent-sdk` - The Claude Code CLI is **automatically bundled** with the package.
- **TypeScript/JavaScript:** Claude Code ships as a **Bun single-file executable** - a self-contained binary that runs anywhere without requiring Node.js or Bun to be pre-installed.

#### What This Means for atomCAD

This simplifies integration significantly:

1. **No Node.js dependency** - Bun executables are self-contained
2. **MIT licensed** - Bun remains open-source and freely redistributable
3. **Smaller footprint** - Bun binaries are smaller and faster than Node.js
4. **Native TypeScript** - Bun runs TypeScript directly without compilation step

#### Recommended Distribution Approach

1. Write the AI agent in TypeScript using the Claude Agent SDK
2. Compile to a Bun single-file executable: `bun build --compile agent.ts --outfile atomcad-ai`
3. Bundle the resulting binary (~50-100MB) with atomCAD releases
4. Flutter spawns this executable and communicates via stdin/stdout JSON-RPC

#### Licensing Considerations

| Component | License | Redistribution |
|-----------|---------|----------------|
| Bun runtime | MIT | Freely redistributable |
| Claude Agent SDK | Anthropic Commercial ToS | Allowed for powering products |
| Claude API usage | Per-token billing | User provides API key or atomCAD subscription |

#### AtomCAD Development vs Distribution

- **Development:** AtomCAD Developers install Bun to iterate on the agent code (`bun run agent.ts`)
- **Distribution:** The compiled single-file executable ships with atomCAD - **users need nothing installed**
