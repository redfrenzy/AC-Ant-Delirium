#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RuleDoc {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub detection_model: &'static str,
    pub failure_semantics: &'static str,
    pub remediation: &'static str,
    pub example: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApiDoc {
    pub path: &'static str,
    pub body: &'static str,
}

pub fn find_rule_doc(id: &str) -> Option<RuleDoc> {
    all_rule_docs().iter().find(|doc| doc.id == id).copied()
}

pub fn find_api_doc(path: &str) -> Option<ApiDoc> {
    all_api_docs().iter().find(|doc| doc.path == path).copied()
}

pub fn all_rule_docs() -> &'static [RuleDoc] {
    &[
        RuleDoc {
            id: "max_nesting",
            name: "Max Nesting",
            category: "structural",
            summary: "Constrains the maximum block-depth of a function body to keep control-flow topology reviewable.",
            detection_model: "The current patrol computes an approximate nesting depth by scanning function bodies and counting '{' and '}' transitions outside comment-only lines. Let d_max be the supremum nesting depth observed in the function body. A violation is emitted when d_max > L, where L is the resolved taxonomy parameter 'max_nesting'. Resolution failures are emitted separately when L cannot be materialized as a numeric scalar.",
            failure_semantics: "A violation means the local control-flow tree has grown beyond the admissible branching depth for the active DNA. This is treated as structural overload: the function may still compile, but its state-space exploration cost for humans and AIs rises superlinearly as nested branches accumulate.",
            remediation: "Reduce effective depth by extracting nested regions into helper functions, replacing branch pyramids with guard clauses, or factoring state transitions into smaller composable steps. The preferred correction is topological flattening, not merely moving braces around.",
            example: Some(
                "If a function reaches depth 5 while the active limit is 3, split inner branches into named helpers or convert nested conditionals into early-return guards so that d_max <= 3.",
            ),
        },
        RuleDoc {
            id: "max_params",
            name: "Max Params",
            category: "structural",
            summary: "Constrains the arity of function signatures to limit interface entropy at the call boundary.",
            detection_model: "For each governed declaration, the patrol estimates signature arity and compares it against the resolved scalar 'max_params'. If n_params > L, the declaration is marked as structurally overloaded. The rule models each parameter as an additional input dimension in the local API manifold.",
            failure_semantics: "A violation means the function interface is carrying too many independent degrees of freedom for the active DNA. High arity usually signals mixed responsibilities, weak encapsulation, or missing aggregate types.",
            remediation: "Compress correlated parameters into a struct, state object, or builder input; split the routine into narrower operations; or move rarely used parameters behind configuration defaults. The goal is reducing interface dimensionality rather than hiding complexity.",
            example: Some("Replace fn build(a, b, c, d, e) with fn build(input: BuildInput) when several parameters describe one coherent domain state."),
        },
        RuleDoc {
            id: "logical_density",
            name: "Logical Density",
            category: "structural",
            summary: "Constrains the amount of atomic executable mass allowed inside a single function body.",
            detection_model: "The rule estimates the number of effective statements in a function and compares that count with the resolved threshold 'logical_density'. Conceptually, if S is the cardinality of atomic statements in the body and S > L, the function exceeds the permitted cognitive density for the active taxonomy path.",
            failure_semantics: "A violation means the function is acting as a high-density concentration of responsibilities. Even without deep nesting, a large statement mass raises review cost, weakens locality of intent, and makes future rewrites more error-prone.",
            remediation: "Partition the function into smaller semantic phases with explicit names. Preserve referential intent by extracting steps that can be reasoned about independently. The correction target is lower statement density per unit, not cosmetic line reduction.",
            example: Some(
                "If a function accumulates 41 effective statements under a limit of 34, split parsing, validation, transformation, and reporting into separate helpers so each unit carries a smaller semantic load.",
            ),
        },
        RuleDoc {
            id: "type_logic",
            name: "Type Logic",
            category: "structural",
            summary: "Enforces that declaration forms are compatible with the active DNA profile resolved from the taxonomy path.",
            detection_model: "For the current file DNA, the rule resolves the admissible declaration grammar from 'type_logic'. It then compares the observed declaration prefix or syntactic form against the allowed set. A mismatch means the declaration belongs to a different architectural layer than the current file identity.",
            failure_semantics: "A violation means the code is semantically mislocated. The declaration shape implies one architectural role, while the enclosing DNA declares another. This is treated as category mismatch rather than style drift.",
            remediation: "Either move the declaration into the DNA that admits its grammar or rewrite the declaration to match the current DNA contract. The preferred fix preserves architectural semantics before syntax.",
            example: Some(
                "A 'pub fn' inside 'unitcore' violates the internal implementation contract when unitcore only admits non-public 'fn' forms. Move it to the parent unit layer or rewrite the API boundary.",
            ),
        },
        RuleDoc {
            id: "else_free",
            name: "Else Free",
            category: "structural",
            summary: "Discourages branch tails based on 'else' so governed functions stay flatter, more local, and easier for humans and AIs to rewrite safely.",
            detection_model: "The patrol resolves the taxonomy flag 'else_free' plus the local policy knobs 'else_free_allow_else_if' and 'else_free_bypass_token'. It then scans each governed function body line-by-line, sanitizes strings and trailing comments, and detects the keyword 'else' as a standalone lexical token. If an 'else' (or 'else if' when not allowed) appears without the configured bypass token in the line comment, a violation is emitted.",
            failure_semantics: "A violation means the function still relies on a tail branch that could often be flattened into guard clauses, smaller helpers, or a more explicit decision structure. This is treated as structural friction: the code may compile and even stay under max_nesting, yet its local rewrite surface becomes less predictable.",
            remediation: "Prefer early returns, extracted helpers, or a clearer branch decomposition. Use the bypass token only when the trailing branch is genuinely the clearest form and the reason needs to remain visible to future editors.",
            example: Some(
                "Replace 'if invalid { ... } else { run(); }' with 'if invalid { return ...; } run();' unless the else branch is intentionally retained with an inline '[REASON]' justification.",
            ),
        },
        RuleDoc {
            id: "doc_tags",
            name: "Doc Tags",
            category: "pedagogical",
            summary: "Aggregates the active pedagogical tag contract of a declaration into a single consolidated audit finding per function.",
            detection_model: "The patrol resolves whether the unified 'doc_tags' mode is enabled and then inspects the documentation contract declared under the nearest taxonomy branch, using child nodes such as 'documentation.what', 'documentation.why', 'documentation.usage', and 'documentation.test'. For each governed function, it applies an internal checklist per configured tag. The current checklist includes tag presence, minimum payload length, and a word-salad heuristic that flags comments whose token mass mostly repeats the symbol name. Instead of emitting one violation per tag, it emits one consolidated violation per function summarizing the incomplete contract.",
            failure_semantics: "A violation means the declaration's pedagogical surface is incomplete for its active DNA. The function may compile and even satisfy structural rules, but the explanatory contract visible to humans and AIs is fragmented or under-informative.",
            remediation: "Rewrite the documentation block so it contains every configured tag for the current DNA with enough semantic payload to satisfy the per-tag minimums. Use the consolidated quick-fix stub as the baseline shape and then replace placeholders with real intent, rationale, usage guidance, and test mapping where applicable.",
            example: Some("If a unit function requires WHAT, WHY, USAGE, and TEST, the aggregated violation may say that [WHY] and [TEST] are missing while [WHAT] is too short, allowing one rewrite pass to repair the full comment block."),
        },
        RuleDoc {
            id: "md_pairing",
            name: "Markdown Pairing",
            category: "pedagogical",
            summary: "In the current implementation, this rule enforces presence of the pedagogical channel '[WHERE]' on governed Rust declarations when the resolved 'md_pairing' policy is enabled.",
            detection_model: "The patrol first resolves whether 'md_pairing' is enabled for the current taxonomy path. If disabled, or if the file is not a Rust source file, the rule is inert. If enabled, it delegates to the shared pedagogical tag verifier and checks each governed function for a '[WHERE]' tag in the attached documentation block. Therefore the operational predicate is documentation-tag presence, not filesystem pairing.",
            failure_semantics: "A violation means the declaration lacks explicit architectural localization. The function may still describe what it does, but it does not encode where it belongs in the system topology. For AI-assisted maintenance, this removes a key coordinate used to map local logic into the broader architecture.",
            remediation: "Add a '[WHERE]' line to the documentation block above the declaration and describe the architectural location, layer, or subsystem boundary where the declaration participates. The correction should be spatially informative, not a restatement of the file name.",
            example: Some("If a governed function has [WHAT] and [WHY] but no [WHERE], add a line such as '/// [WHERE] Structural validation layer for unit-level API boundaries.'"),
        },
        RuleDoc {
            id: "md_min_chars",
            name: "Markdown Minimum Size",
            category: "pedagogical",
            summary: "Applies a lower bound to documentation information volume so trivial or degenerate docs do not masquerade as valid guidance.",
            detection_model: "The rule measures documentation payload length and compares it to the resolved lower bound 'md_min_chars'. If |doc| < L, the document is treated as under-informative. The metric is coarse but intentionally simple: it approximates whether the explanation has enough informational mass to be useful.",
            failure_semantics: "A violation means the document exists but is too small to serve as an effective explanatory artifact. This is not absence; it is subcritical documentation density.",
            remediation: "Add mission, rationale, context, constraints, or usage information until the document encodes enough operational meaning to cross the minimum threshold.",
            example: None,
        },
        RuleDoc {
            id: "what_mission",
            name: "What Mission",
            category: "pedagogical",
            summary: "Requires an explicit [WHAT] semantic tag describing the observable purpose of the declaration.",
            detection_model: "The patrol inspects the documentation block attached to a governed declaration and searches for the required [WHAT] channel. Depending on the implementation path, the rule may also enforce a minimum descriptive payload rather than mere tag existence.",
            failure_semantics: "A violation means the declaration exposes behavior without a normalized purpose statement. For AI-assisted maintenance, this removes the shortest path from syntax to intent.",
            remediation: "Add a [WHAT] line that describes what the declaration does at the externally relevant level, avoiding tautology and low-information restatement of the function name.",
            example: None,
        },
        RuleDoc {
            id: "why_justification",
            name: "Why Justification",
            category: "pedagogical",
            summary: "Requires an explicit [WHY] channel that encodes the design motive or architectural necessity of the declaration.",
            detection_model: "The rule inspects the attached doc block for a [WHY] field and treats absence or insufficient content as a pedagogical defect. The target variable is design rationale, not behavioral summary.",
            failure_semantics: "A violation means the code communicates mechanism without justification. This breaks causal traceability: future editors can see what exists but not why the system chose this form.",
            remediation: "State the design pressure, architectural constraint, or system-level reason that makes the declaration necessary. The correction should answer 'why this exists here' rather than 'what it does'.",
            example: None,
        },
        RuleDoc {
            id: "usage_example",
            name: "Usage Example",
            category: "pedagogical",
            summary: "Requires an explicit [USAGE] channel describing operational invocation, verification, or interaction semantics.",
            detection_model: "The rule parses the documentation block and checks whether the [USAGE] slot exists with enough actionable guidance to connect the declaration to a valid execution or verification pattern.",
            failure_semantics: "A violation means readers can identify purpose and rationale but still lack an execution model. This increases misuse probability and weakens AI prompt grounding during edits.",
            remediation: "Add a [USAGE] line or paragraph describing how the declaration should be called, observed, or validated in practice.",
            example: None,
        },
        RuleDoc {
            id: "test_case_mapping",
            name: "Test Case Mapping",
            category: "pedagogical",
            summary: "Requires an explicit [TEST] mapping between the declaration and its intended validation strategy.",
            detection_model: "The rule inspects the documentation block for a [TEST] tag and treats its absence as missing verification traceability. The expected payload may reference a concrete test, scenario, or verification mechanism.",
            failure_semantics: "A violation means the declaration lacks an explicit audit trail to its verification surface. This weakens maintainability because regressions become harder to localize and reproduce.",
            remediation: "Document the test name, scenario, contract, or validation method that should prove the declaration correct.",
            example: None,
        },
        RuleDoc {
            id: "root_md_pairing",
            name: "Root Markdown Pairing",
            category: "pedagogical",
            summary: "Requires root-scoped governed artifacts to expose a root markdown guide when the active policy demands one.",
            detection_model: "This is a root-level existence constraint analogous to md_pairing, but applied to special project anchors rather than ordinary sibling files. The rule resolves the expected root guide path and emits a violation when the artifact is absent.",
            failure_semantics: "A violation means the project has lost its top-level pedagogical anchor, which is often the first document used to orient humans and AIs before traversing deeper modules.",
            remediation: "Create the expected root markdown guide in the resolved location and encode project-level guidance, boundaries, or navigation information there.",
            example: None,
        },
        RuleDoc {
            id: "inheritance_visibility",
            name: "Inheritance Visibility",
            category: "pedagogical",
            summary: "Ensures that required inherited documentation remains visible across the governed hierarchy instead of being shadowed or silently dropped.",
            detection_model: "The rule checks whether a declaration or file that should expose inherited pedagogical context still carries the visibility required by the active hierarchy. In effect, it validates continuity of inherited documentation signals across layers.",
            failure_semantics: "A violation means part of the contextual chain has been occluded. The implementation still exists, but one or more inherited explanatory invariants are no longer observable at the required layer.",
            remediation: "Restore the missing inherited documentation signal or reposition the declaration so the documentation contract is satisfied at the correct hierarchy level.",
            example: None,
        },
        RuleDoc {
            id: "dna_suffix",
            name: "DNA Suffix",
            category: "identity",
            summary: "Constrains file names so their suffix encodes the correct DNA identity declared by the active taxonomy.",
            detection_model: "The rule parses the terminal portion of the file stem and compares it against the admissible DNA suffix set. It distinguishes identity mismatch from mere file existence: the artifact may be present, but its encoded type label is invalid or ambiguous.",
            failure_semantics: "A violation means the file's lexical identity disagrees with the architecture's type system. This degrades taxonomy resolution, search reliability, and AI inference about the file's intended role.",
            remediation: "Rename the file so its suffix matches a valid DNA for the current branch. Prefer exact identity alignment over approximate similarity.",
            example: Some(
                "Rename Feature_unite.rs to Feature_unitcore.rs when the active branch admits 'unitcore' and rejects 'unite'.",
            ),
        },
        RuleDoc {
            id: "tag_separator",
            name: "Tag Separator",
            category: "identity",
            summary: "Enforces the separator used between semantic tags inside governed identifiers.",
            detection_model: "The rule inspects the lexical separator used in file stems and compares it to the resolved taxonomy scalar 'tag_separator'. Any competing delimiter in positions reserved for tag boundaries is treated as an identity violation.",
            failure_semantics: "A violation means the identifier grammar is inconsistent with the active naming algebra. Even small delimiter drift can fragment tooling assumptions and taxonomy lookups.",
            remediation: "Rename the file so all tag boundaries use the configured separator exactly once and consistently.",
            example: None,
        },
        RuleDoc {
            id: "pascal_case",
            name: "Pascal Case",
            category: "identity",
            summary: "Enforces the active casing convention for governed identifiers.",
            detection_model: "The rule compares the observed lexical case pattern against the configured casing model, typically PascalCase in the current hierarchy. The predicate is evaluated on the identifier surface rather than semantic content.",
            failure_semantics: "A violation means the identifier no longer matches the case grammar expected by the project. This weakens naming regularity and can make generated or inferred names less stable.",
            remediation: "Rename the file or declaration so the identifier follows the configured case transformation exactly.",
            example: None,
        },
        RuleDoc {
            id: "recursion",
            name: "Recursion",
            category: "identity",
            summary: "Validates whether the file layout respects the recursive jurisdiction policy declared by the hierarchy.",
            detection_model: "The rule compares the physical path of the artifact against the recursive scope admitted by the resolved taxonomy branch. It treats path topology as part of identity: the file is valid only if its containment relation satisfies the recursion policy.",
            failure_semantics: "A violation means the file exists in an inadmissible topological region of the project tree. The identity is therefore spatially inconsistent even if naming is otherwise valid.",
            remediation: "Move the file into the correct recursive jurisdiction or adjust the hierarchy definition if the intended topology has genuinely changed.",
            example: None,
        },
        RuleDoc {
            id: "manifest_anchoring",
            name: "Manifest Anchoring",
            category: "identity",
            summary: "Requires the source artifact to remain anchored to its expected manifest companion when the active DNA uses manifest support.",
            detection_model: "The rule derives the expected manifest artifact from the source path and verifies that the anchoring relation still holds. This is an existence-plus-correspondence constraint, not just a generic file lookup.",
            failure_semantics: "A violation means the code artifact has become detached from its manifest metadata surface. This reduces machine-readable introspection quality and weakens AI context recovery.",
            remediation: "Generate or restore the missing manifest artifact and ensure the naming/path correspondence remains stable with the source file.",
            example: None,
        },
        RuleDoc {
            id: "name_separator",
            name: "Name Separator",
            category: "identity",
            summary: "Enforces the separator used between human-readable name segments in governed identifiers.",
            detection_model: "The rule inspects the file stem and compares its segment delimiter against the configured scalar 'name_separator'. This is distinct from tag_separator: it constrains the lexical segmentation of the semantic name itself.",
            failure_semantics: "A violation means the identifier's human-readable segmentation grammar is inconsistent with project policy, which harms predictability and canonical name generation.",
            remediation: "Rename the file so semantic name segments use the configured separator consistently throughout the stem.",
            example: None,
        },
        RuleDoc {
            id: "repository_anchor",
            name: "Repository Anchor",
            category: "identity",
            summary: "Validates that repository-scoped references remain aligned with the configured repository anchor.",
            detection_model: "The rule compares the observed repository reference against the resolved anchor path stored in the active hierarchy. It treats anchor mismatch as an identity-resolution defect because path-based semantics become ambiguous when the repository root drifts.",
            failure_semantics: "A violation means the artifact references the wrong repository base or an obsolete anchor. The project may still be parseable, but environment-level identity has become inconsistent.",
            remediation: "Point the reference back to the configured repository anchor or revise the hierarchy if the repository root intentionally changed.",
            example: None,
        },
        RuleDoc {
            id: "restricted_tokens",
            name: "Restricted Tokens",
            category: "sanitization",
            summary: "Detects lexically forbidden tokens declared unsafe, noisy, or architecturally banned by the active policy.",
            detection_model: "The patrol scans governed source text for token occurrences that belong to the resolved restricted set. For each match t in T_forbidden, it emits a violation with the configured hint and, when available, a quick-fix direction. This is a lexical sanitization rule with policy-driven semantics.",
            failure_semantics: "A violation means the code contains an operation or lexical construct explicitly excluded by project governance. The token may be legal Rust, but it is considered locally impure or unsafe for this codebase.",
            remediation: "Replace the token with the sanctioned alternative described by policy, typically an explicit Result path, boundary abstraction, or approved reporting mechanism.",
            example: Some("Replace unwrap with explicit error handling or an approved adapter."),
        },
        RuleDoc {
            id: "allowed_crates",
            name: "Allowed Crates",
            category: "dependency_governance",
            summary: "Constrains the dependency alphabet to the set explicitly admitted by the active hierarchy.",
            detection_model: "The rule extracts external crate usages from the governed source and compares each imported crate c against the allowed set A. If c ∉ A, the dependency is classified as unauthorized under the current policy.",
            failure_semantics: "A violation means the code depends on an external component outside the permitted dependency manifold. This is a policy impurity, not necessarily a compile error.",
            remediation: "Remove the unauthorized crate, replace it with an allowed equivalent, or extend the allow-list only if the architectural justification is explicit and accepted.",
            example: None,
        },
        RuleDoc {
            id: "strict_wrapper_boundary",
            name: "Strict Wrapper Boundary",
            category: "dependency_governance",
            summary: "Forces src/libres wrappers to expose only explicit dependency boundaries instead of full third-party crate surfaces.",
            detection_model: "The rule inspects src/libres wrappers, rejects wildcard/full forwarding such as `pub use crate_name::*;`, and requires explicit AC_* aliases so only selected items cross the boundary.",
            failure_semantics: "A violation means the wrapper leaked too much external API or failed to publish a canonical AC_* boundary, increasing blast radius and coupling in call sites.",
            remediation: "Expose only the exact used items and map them to AC_* aliases. Pattern: `pub use crate_name::{TypeA, TypeB}; pub type AC_TypeA = TypeA; pub type AC_TypeB = TypeB;`.",
            example: Some("Std-like boundary: `pub use std::{string::String, vec::Vec}; pub type AC_String = String; pub type AC_Vec<T> = Vec<T>;`."),
        },
        RuleDoc {
            id: "token_sanitization",
            name: "Token Sanitization",
            category: "sanitization",
            summary: "Checks whether sensitive lexical constructs are isolated or normalized according to the active sanitization model.",
            detection_model: "Where restricted_tokens detects direct prohibition, token_sanitization models whether a tokenized pattern appears in an admissible containment regime. It is a lexical-boundary rule: unsafe constructs must be absent or quarantined behind permitted abstractions.",
            failure_semantics: "A violation means a sensitive construct leaks into governed code instead of remaining isolated behind a controlled interface or approved layer.",
            remediation: "Relocate the construct into a sanctioned abstraction boundary, rewrite the code path, or remove the construct entirely from the governed region.",
            example: None,
        },
        RuleDoc {
            id: "super_block",
            name: "Super Block",
            category: "sanitization",
            summary: "Protects critical governance assets before the normal audit starts, including build.rs hook order and filesystem hardening for the shared guard and plugin package.",
            detection_model: "The rule runs as a preflight before the file sweep. It validates that configured protected files exist, satisfy the expected owner/readonly policy, and that each protected build.rs calls the standardized guard as the first operational statement inside fn main().",
            failure_semantics: "An enforced violation means the build-governance chain can be bypassed or rewritten before the audit executes. Watch-level findings mean the project is drifting around sensitive assets and should be corrected before the safety boundary erodes further.",
            remediation: "Restore the standardized guard, call it first inside each protected build.rs, and lock the protected files to the owner and readonly policy declared in hierarchy.super_block.",
            example: Some("If build.rs starts with project-specific logic before ant_delirium_build_guard::run(...), move the guard call to the first executable statement and lock the guard/TOML files read-only."),
        },
        RuleDoc {
            id: "dependency_purification",
            name: "Dependency Purification",
            category: "dependency_governance",
            summary: "Enforces purification constraints on dependency edges so impurity is not propagated into governed layers.",
            detection_model: "The rule evaluates dependency usage against the active purification policy, treating each forbidden edge as a contamination event in the local dependency graph. If a source node imports an impure external node without an approved boundary, a violation is emitted.",
            failure_semantics: "A violation means impurity has crossed a layer boundary that should remain clean under the architecture. This is graph contamination rather than simple crate-list drift.",
            remediation: "Remove the impure edge, route it through an approved anti-corruption boundary, or refactor the dependency so clean layers remain topologically pure.",
            example: None,
        },
    ]
}

pub fn all_api_docs() -> &'static [ApiDoc] {
    &[
        ApiDoc {
            path: "hierarchy.installation",
            body: "This is a project-scoped installation node under the active taxonomy root. Direct plugin information belongs to `<plugin-binary> --installation` and `<plugin-binary> --plugin.help`. The supported layout keeps the project-local binary at `plugins/<project>/bin/<plugin-binary>` and the authoritative TOML package at `plugins/<project>/ant_delirium_config`, beside the bin directory, so multiple branches and worktrees reuse the same governance package.",
        },
        ApiDoc {
            path: "hierarchy.installation.build",
            body: "The target crate `build.rs` should resolve `plugins/<project>/bin/<plugin-binary>` plus `plugins/<project>/ant_delirium_config`, call the standardized guard as the first operational statement inside `fn main()`, and stop compilation when the guard returns a non-zero status. Keep the TOMLs outside the crate, beside the final binary package, instead of copying them into every branch or worktree.",
        },
        ApiDoc {
            path: "hierarchy.installation.api",
            body: "Use direct binary commands for plugin operations such as `<plugin-binary> --installation`, `<plugin-binary> --plugin.help`, `<plugin-binary> config init`, `<plugin-binary> swarm status`, `<plugin-binary> --help.feedback`, and `<plugin-binary> --feedback \"...\" --context <taxonomy.path>`. Use `--api <root>...` only for project taxonomy commands such as `<plugin-binary> --api hierarchy.installation` and `<plugin-binary> --api hierarchy.dependency_governance.allowed_crates`.",
        },
        ApiDoc {
            path: "hierarchy.installation.security",
            body: "Treat `build.rs`, `AC_Ant-Delirium_props/ant_delirium_build_guard.rs`, the plugin binary, and the external TOML configuration as controlled assets. The recommended model is root-owned or equivalent superuser-controlled readonly locking for enforced files declared under `hierarchy.super_block`, while secondary watch files remain observable but less strict.",
        },
        ApiDoc {
            path: "hierarchy.installation.context",
            body: "The active plugin writes context.json with the repository root, current workspace, and plugin location so each agent can reason about its own execution boundary.",
        },
        ApiDoc {
            path: "hierarchy.installation.agents",
            body: "Connected agents are stored in connected.json. Use `<plugin-binary> swarm status` to inspect active workspaces, assigned missions, and aggregated violation counts.",
        },
        ApiDoc {
            path: "hierarchy.installation.messages",
            body: "Agent-to-agent communication is stored in messages.json. Use `<plugin-binary> --message \"...\" --to-agent <id>` for a directed instruction, `--from-agent <id>` when you must override sender identity, and `<plugin-binary> --broadcast \"...\"` for a global message. Use `<plugin-binary> swarm status` to inspect active agents before routing work.",
        },
        ApiDoc {
            path: "hierarchy.installation.feedback",
            body: "Human feedback is stored in feedback.json. Use `<plugin-binary> --feedback \"...\" --context <taxonomy.path>` to record confusion points, missing explanations, or governance problems tied to a specific taxonomy node. Use `<plugin-binary> --help.feedback` for the canonical pattern.",
        },
        ApiDoc {
            path: "rules.installation",
            body: "This is a project-scoped installation node under the active taxonomy root. Direct plugin information belongs to `<plugin-binary> --installation` and `<plugin-binary> --plugin.help`. The supported layout keeps the project-local binary at `plugins/<project>/bin/<plugin-binary>` and the authoritative TOML package at `plugins/<project>/ant_delirium_config`, beside the bin directory, so multiple branches and worktrees reuse the same governance package.",
        },
        ApiDoc {
            path: "rules.installation.build",
            body: "The target crate `build.rs` should resolve `plugins/<project>/bin/<plugin-binary>` plus `plugins/<project>/ant_delirium_config`, call the standardized guard as the first operational statement inside `fn main()`, and stop compilation when the guard returns a non-zero status. Keep the TOMLs outside the crate, beside the final binary package, instead of copying them into every branch or worktree.",
        },
        ApiDoc {
            path: "rules.installation.api",
            body: "Use direct binary commands for plugin operations such as `<plugin-binary> --installation`, `<plugin-binary> --plugin.help`, `<plugin-binary> config init`, `<plugin-binary> swarm status`, `<plugin-binary> --help.feedback`, and `<plugin-binary> --feedback \"...\" --context <taxonomy.path>`. Use `--api <root>...` only for project taxonomy commands such as `<plugin-binary> --api rules.installation` and `<plugin-binary> --api rules.dependency_governance.allowed_crates`.",
        },
        ApiDoc {
            path: "rules.installation.security",
            body: "Treat `build.rs`, `AC_Ant-Delirium_props/ant_delirium_build_guard.rs`, the plugin binary, and the external TOML configuration as controlled assets. The recommended model is root-owned or equivalent superuser-controlled readonly locking for enforced files declared under `rules.super_block`, while secondary watch files remain observable but less strict.",
        },
        ApiDoc {
            path: "rules.installation.context",
            body: "The active plugin writes context.json with the repository root, current workspace, and plugin location so each agent can reason about its own execution boundary.",
        },
        ApiDoc {
            path: "rules.installation.agents",
            body: "Connected agents are stored in connected.json. Use `<plugin-binary> swarm status` to inspect active workspaces, assigned missions, and aggregated violation counts.",
        },
        ApiDoc {
            path: "rules.installation.messages",
            body: "Agent-to-agent communication is stored in messages.json. Use `<plugin-binary> --message \"...\" --to-agent <id>` for a directed instruction, `--from-agent <id>` when you must override sender identity, and `<plugin-binary> --broadcast \"...\"` for a global message. Use `<plugin-binary> swarm status` to inspect active agents before routing work.",
        },
        ApiDoc {
            path: "rules.installation.feedback",
            body: "Human feedback is stored in feedback.json. Use `<plugin-binary> --feedback \"...\" --context <taxonomy.path>` to record confusion points, missing explanations, or governance problems tied to a specific taxonomy node. Use `<plugin-binary> --help.feedback` for the canonical pattern.",
        },
        ApiDoc {
            path: "regras.installation",
            body: "Este no de instalacao e da API do projeto, nao do comando direto do plugin. As informacoes diretas do plugin ficam em `<plugin-binary> --installation` e `<plugin-binary> --plugin.help`. O layout suportado mantem o binario final em `plugins/<projeto>/bin/<plugin-binary>` e o pacote TOML autoritativo em `plugins/<projeto>/ant_delirium_config`, ao lado do diretorio bin, para reutilizacao entre branches e worktrees.",
        },
        ApiDoc {
            path: "regras.installation.build",
            body: "O `build.rs` do crate alvo deve resolver `plugins/<projeto>/bin/<plugin-binary>` e `plugins/<projeto>/ant_delirium_config`, chamar o guard padronizado como a primeira instrucao operacional dentro de `fn main()`, e interromper a compilacao quando o guard retornar status diferente de zero. Os TOMLs devem ficar fora do crate, ao lado do pacote binario final, e nao copiados para cada branch ou worktree.",
        },
        ApiDoc {
            path: "regras.installation.api",
            body: "Use comandos diretos do plugin para operacoes do proprio plugin, como `<plugin-binary> --installation`, `<plugin-binary> --plugin.help`, `<plugin-binary> config init`, `<plugin-binary> swarm status`, `<plugin-binary> --help.feedback` e `<plugin-binary> --feedback \"...\" --context <taxonomy.path>`. Use `--api <root>...` apenas para comandos da taxonomia do projeto, como `<plugin-binary> --api regras.installation` e `<plugin-binary> --api regras.dependency_governance.allowed_crates`.",
        },
        ApiDoc {
            path: "regras.installation.security",
            body: "Trate `build.rs`, `AC_Ant-Delirium_props/ant_delirium_build_guard.rs`, o binario do plugin e a configuracao TOML externa como ativos controlados. O modelo recomendado e travar os arquivos enforced de `regras.super_block` como somente leitura sob controle de superusuario, enquanto arquivos watch permanecem apenas monitorados.",
        },
        ApiDoc {
            path: "regras.installation.context",
            body: "O plugin ativo grava context.json com a raiz do repositorio, o workspace atual e a localizacao do plugin para que cada agente entenda seu proprio limite de execucao.",
        },
        ApiDoc {
            path: "regras.installation.agents",
            body: "Os agentes conectados sao armazenados em connected.json. Use `<plugin-binary> swarm status` para inspecionar workspaces ativos, missoes atribuidas e contagem agregada de violacoes.",
        },
        ApiDoc {
            path: "regras.installation.messages",
            body: "A comunicacao entre agentes fica em messages.json. Use `<plugin-binary> --message \"...\" --to-agent <id>` para uma instrucao direcionada, `--from-agent <id>` quando precisar sobrescrever a identidade do remetente, e `<plugin-binary> --broadcast \"...\"` para uma mensagem global. Use `<plugin-binary> swarm status` para inspecionar os agentes ativos antes de rotear trabalho.",
        },
        ApiDoc {
            path: "regras.installation.feedback",
            body: "O feedback humano fica em feedback.json. Use `<plugin-binary> --feedback \"...\" --context <taxonomy.path>` para registrar pontos de confusao, falta de explicacao ou problemas de governanca ligados a um no taxonomico especifico. Use `<plugin-binary> --help.feedback` para o formato canonico.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::{find_api_doc, find_rule_doc, ApiDoc, RuleDoc};

    #[test]
    fn finds_known_rule_doc() {
        let rule = find_rule_doc("md_pairing");
        assert_eq!(
            rule,
            Some(RuleDoc {
                id: "md_pairing",
                name: "Markdown Pairing",
                category: "pedagogical",
                summary: "In the current implementation, this rule enforces presence of the pedagogical channel '[WHERE]' on governed Rust declarations when the resolved 'md_pairing' policy is enabled.",
                detection_model: "The patrol first resolves whether 'md_pairing' is enabled for the current taxonomy path. If disabled, or if the file is not a Rust source file, the rule is inert. If enabled, it delegates to the shared pedagogical tag verifier and checks each governed function for a '[WHERE]' tag in the attached documentation block. Therefore the operational predicate is documentation-tag presence, not filesystem pairing.",
                failure_semantics: "A violation means the declaration lacks explicit architectural localization. The function may still describe what it does, but it does not encode where it belongs in the system topology. For AI-assisted maintenance, this removes a key coordinate used to map local logic into the broader architecture.",
                remediation: "Add a '[WHERE]' line to the documentation block above the declaration and describe the architectural location, layer, or subsystem boundary where the declaration participates. The correction should be spatially informative, not a restatement of the file name.",
                example: Some("If a governed function has [WHAT] and [WHY] but no [WHERE], add a line such as '/// [WHERE] Structural validation layer for unit-level API boundaries.'"),
            })
        );
    }

    #[test]
    fn finds_known_api_doc() {
        let doc = find_api_doc("hierarchy.installation");
        assert_eq!(
            doc,
            Some(ApiDoc {
                path: "hierarchy.installation",
                body: "This is a project-scoped installation node under the active taxonomy root. Direct plugin information belongs to `<plugin-binary> --installation` and `<plugin-binary> --plugin.help`. The supported layout keeps the project-local binary at `plugins/<project>/bin/<plugin-binary>` and the authoritative TOML package at `plugins/<project>/ant_delirium_config`, beside the bin directory, so multiple branches and worktrees reuse the same governance package.",
            })
        );
    }
}
