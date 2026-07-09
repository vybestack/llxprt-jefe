# Component 002 Pseudocode — GitHub Client Boundary (gh CLI) for Pull Requests

Plan ID: `PLAN-20260624-PR-MODE`

Requirements: REQ-PR-006,007,009,010,011,012,013, REQ-PR-NFR-001,002,003

This component is the isolated `gh` CLI wrapper boundary (`src/github/mod.rs` + `parse_pr.rs`).
It depends ONLY on `crate::domain`. It does not import `crate::state`, `crate::ui`, or
`crate::app_input`, and it never mutates `AppState`. Every method is synchronous and returns a
typed `Result<_, GhError>`; the dispatch layer runs these methods off the UI thread via
`spawn_gh_task_with_panic`. `GhError`, `parse_comments_json`, `parse_page_info`,
`parse_created_comment_json`, and the `IssueComment` domain type are reused from the existing client.
PR comments are FETCHED via a NEW PR-specific `list_pr_comments` method (`repository.pullRequest`),
NOT the existing issue `list_comments` (`repository.issue`), because `repository.issue(number:)` is
NULL for a PR number — see `list_pr_comments` (lines 102-107) and the "PR detail comments" note below.

## statusCheckRollup JSON shape (grounded; see P00A capture step)

`statusCheckRollup` is a HETEROGENEOUS array of check entries with TWO distinct shapes
(`__typename`-discriminated). The parser MUST handle BOTH and never drop an entry:

| Shape (`__typename`) | name field | status fields | url field |
|----------------------|------------|---------------|-----------|
| `CheckRun` (Actions/Apps) | `name` | `status` (QUEUED\|IN_PROGRESS\|COMPLETED\|WAITING\|PENDING\|REQUESTED) + `conclusion` (SUCCESS\|FAILURE\|NEUTRAL\|SKIPPED\|CANCELLED\|TIMED_OUT\|ACTION_REQUIRED\|STALE\|STARTUP_FAILURE) | `detailsUrl` |
| `StatusContext` (legacy commit status) | `context` | `state` (EXPECTED\|ERROR\|FAILURE\|PENDING\|SUCCESS) | `targetUrl` |

Canonical normalization (mirrors the `gh` CLI's own `--jq` idiom `.context // .name` and
`.state // .conclusion`): name = `node.name` else `node.context`; raw status token =
`node.conclusion` else `node.state` else `node.status`; url = `node.detailsUrl` else `node.targetUrl`.

- `gh pr view <n> --json statusCheckRollup` returns a FLAT array: `statusCheckRollup: [ {CheckRun…} | {StatusContext…} ]` (each element carries `__typename`).
- `gh api graphql ... search(...) { ... on PullRequest { statusCheckRollup { __typename ... } } }`
  exposes the rollup as `statusCheckRollup` (a `StatusCheckRollupContext` connection); the list query
  selects `statusCheckRollup { contexts(first: 100) { nodes { __typename ... on CheckRun {...} ... on StatusContext {...} } } }`.
- The parser reads `node.__typename` to disambiguate but ALSO falls back to field presence
  (`name`/`context`, `conclusion`/`state`) so a missing/unknown `__typename` still yields a
  displayable degraded record rather than a dropped one (REQ-PR-013).

```text
01: STRUCT GhClient (unit; Clone, Copy, Debug)        // reuse existing type
02:   // no fields; methods shell out to the `gh` binary

03: REUSE ENUM GhError { NotInstalled, NotAuthenticated, RateLimited, AccessDenied,
04:                      ApiError(String), ParseError(String), NetworkError(String) }

05: STRUCT PrListResponse { pull_requests: Vec<PullRequest>, cursor: Option<String>,
06:                        has_more: bool }

07: CONST PR_LIST_PAGE_SIZE = 30
08: CONST PR_COMMENT_PAGE_SIZE = 30                    // reuse issue comment page size

09: FUNCTION validate_repo_slug(slug) -> Result<(owner, name), GhError>
10:   IF slug not matching ^[^/\s]+/[^/\s]+$ THEN RETURN Err(ApiError("invalid repo slug"))
11:   RETURN Ok(split slug on '/')

12: FUNCTION run_gh(args) -> Result<stdout, GhError>   // shared error idiom (mirror existing)
13:   output = Command::new("gh").args(args).output()
14:   MAP Err(e):
15:     IF e.kind == NotFound THEN RETURN Err(NotInstalled)
16:     ELSE RETURN Err(NetworkError(e.to_string()))
17:   IF NOT output.status.success():
18:     RETURN Err(categorize_error(output.status.code, output.stderr))  // reuse categorize_error
19:   RETURN Ok(output.stdout as string)

20: FUNCTION check_auth(client) -> Result<(), GhError>  // may reuse existing check_auth
21:   run_gh(["auth", "status"]) -> map non-zero to NotAuthenticated

22: FUNCTION list_pull_requests(client, owner, name, filter, cursor) -> Result<PrListResponse>
23:   VALIDATE (owner,name) via validate_repo_slug upstream
24:   args = build_pr_search_args(owner, name, filter, cursor, PR_LIST_PAGE_SIZE)
25:     // REAL cursor pagination — mirrors the ACTIVE issues list path
26:     // (src/github/parse.rs::build_issue_search_args L594-621,
27:     //  src/github/mod.rs::list_issues L134-159).
28:     // Uses `gh api graphql` with search(type: ISSUE, query, first, after) + pageInfo,
29:     // NOT `gh pr list` (which exposes only `--limit` — no cursor/page flag).
30:   stdout = run_gh(args)?
31:   parsed = parse_pull_requests_json(stdout)?       // see parse helpers
32:   sorted = sort_pull_requests(parsed.items)        // updated desc, number asc tiebreak
33:   RETURN Ok(PrListResponse{ pull_requests: sorted, cursor: parsed.next_cursor,
34:                            has_more: parsed.has_more })

35: FUNCTION build_pr_search_args(owner, name, filter, cursor, page_size) -> Vec<String>
36:   // Mirror src/github/parse.rs::build_issue_search_args exactly, with a PR query + PR fields.
37:   // The `... on PullRequest` selection includes `state` (the GraphQL PullRequestState enum,
38:   // which IS one of OPEN|CLOSED|MERGED directly) AND `mergedAt` (Finding 3): both are requested
39:   // so the parser can map merged PRs to PrState::Merged whether the API reports state=MERGED or
40:   // only a non-null mergedAt. statusCheckRollup is selected as a `contexts(first:100).nodes`
41:   // connection with BOTH CheckRun and StatusContext inline fragments (Finding 8).
42:   PR_FIELDS = "number title state mergedAt author { login } updatedAt headRefName baseRefName \
43:                isDraft reviewDecision \
44:                statusCheckRollup { contexts(first: 100) { nodes { __typename \
45:                  ... on CheckRun { name status conclusion detailsUrl } \
46:                  ... on StatusContext { context state targetUrl } } } } \
47:                assignees(first: 10) { nodes { login } } \
48:                labels(first: 20) { nodes { name } } comments { totalCount } body"
49:   query_with_after  = "query($query: String!, $first: Int!, $after: String) { \
50:                          search(type: ISSUE, query: $query, first: $first, after: $after) { \
51:                            nodes { ... on PullRequest { " + PR_FIELDS + " } } \
52:                            pageInfo { hasNextPage endCursor } } }"
53:   query_first_page  = same as above WITHOUT the `$after`/`after:` clauses
54:   q = build_pr_search_query(owner, name, filter)
55:   args = ["api","graphql","-f", "query=" + (cursor ? query_with_after : query_first_page),
56:           "-F", "query=" + q, "-F", "first=" + page_size]
57:   IF cursor present THEN args += ["-F", "after=" + cursor]
58:   RETURN args

59: FUNCTION build_pr_search_query(owner, name, filter) -> String
60:   // Pure GitHub search-qualifier string (mirror issue_search_query in parse.rs L561-578).
61:   terms = ["repo:" + owner + "/" + name, "is:pr"]
62:   MATCH filter.state:
63:     Some(Open)   -> terms += "is:open"
64:     Some(Closed) -> terms += "is:closed"
65:     Some(Merged) -> terms += "is:merged"
66:     Some(All) | None -> (omit state qualifier)
67:   FOR label in filter.labels: terms += "label:" + label
68:   IF filter.author   nonempty: terms += "author:" + author
69:   IF filter.assignee nonempty: terms += "assignee:" + assignee
70:   IF filter.reviewer nonempty: terms += "review-requested:" + reviewer
71:   MATCH filter.is_draft: Some(true) -> terms += "draft:true"; Some(false) -> terms += "draft:false"; None -> ()
71a:    // Finding 3: `draft:true`/`draft:false` is the VERIFIED server-side qualifier for the
71b:    //   `search(type: ISSUE, query:...)` GraphQL endpoint used here (P00A §2b proves: draft:true
71c:    //   -> only isDraft=true, draft:false -> only isDraft=false, against cli/cli at plan time).
71d:    //   It filters SERVER-SIDE so endCursor/hasNextPage pagination is preserved (NO client-side
71e:    //   post-filter). `is:draft` is only a one-sided alias (no negation), so it is NOT used.
71f:   MATCH filter.review_decision:                     // Finding 1 — issue #20 review-signal filter
71g:     Approved         -> terms += "review:approved"
71h:     ChangesRequested -> terms += "review:changes_requested"
71i:     ReviewRequired   -> terms += "review:required"
71j:     None             -> terms += "review:none"
71k:     Any              -> ()                            // omit qualifier (do not filter on review)
71l:   MATCH filter.checks_status:                        // Finding 1 — issue #20 workflow-signal filter
71m:     Success -> terms += "status:success"
71n:     Failing -> terms += "status:failure"
71o:     Pending -> terms += "status:pending"
71p:     Any     -> ()                                     // omit qualifier (do not filter on checks)
71q:    // Finding 1: review:*/status:* are VERIFIED server-side qualifiers for the same
71r:    //   `search(type: ISSUE, query:...)` GraphQL endpoint (P00A §2c). They filter SERVER-SIDE so
71s:    //   endCursor/hasNextPage pagination is preserved (NO client-side post-filter). Qualifier
71t:    //   ORDER is deterministic: state, labels, author, assignee, reviewer, draft, review, checks,
71u:    //   then the free-text query — so query construction is stable and testable.
72:   IF filter.query_text.trim() nonempty: terms += query_text.trim()
73:   RETURN join(terms, " ")

74: FUNCTION get_pull_request_detail(client, owner, name, number) -> Result<PullRequestDetail>
75:   args = ["pr", "view", number, "--repo", owner/name, "--json",
76:           "number,title,state,author,createdAt,updatedAt,headRefName,baseRefName,isDraft,
77:            labels,assignees,milestone,body,url,mergedAt,reviewDecision,statusCheckRollup,
78:            reviews"]
79:     // NOTE: this PR --json set OMITS `comments` on purpose; comments are fetched SEPARATELY
80:     //       below via a paginated list_pr_comments call. This mirrors the COMMENTS-SOURCING
81:     //       precedent of src/github/mod.rs::get_issue_detail, NOT its exact --json field list:
82:     //       get_issue_detail's --json list DOES include `comments` (mod.rs L180), parses it
83:     //       (L197), then DELIBERATELY OVERWRITES detail.comments/comments_cursor/
84:     //       has_more_comments via a distinct paginated list_comments call (mod.rs L198-202)
85:     //       so pagination is driven by the real GraphQL cursor. PR detail skips the embedded
86:     //       `comments` field entirely (it would only be overwritten) and relies on the same
87:     //       separate fetch for the first comment page — but via list_pr_comments
87a:    //       (repository.pullRequest(number:).comments), NOT the issue list_comments path, since
87b:    //       repository.issue(number:) is NULL for a PR number (P00A §2d).
88:     // NOTE: `gh pr view --json statusCheckRollup` returns a FLAT array of CheckRun/StatusContext
89:     //       objects (see the shape table above + P00A capture); `mergedAt` is included so the
90:     //       parser can distinguish Merged from Closed (Finding 3).
91:   stdout = run_gh(args)?
92:   detail = parse_pull_request_detail_json(stdout, owner/name)?
93:   // SEPARATE first-page comments fetch with REAL cursor pagination, exactly like get_issue_detail
93a:  // but using the PR-specific comments method (repository.pullRequest, NOT repository.issue):
94:   comments_response = list_pr_comments(client, owner, name, number, None, PR_COMMENT_PAGE_SIZE)?
95:   detail.comments          = comments_response.comments
96:   detail.comments_cursor   = comments_response.cursor
97:   detail.has_more_comments = comments_response.has_more
98:   RETURN Ok(detail)
99:   // detail.reviews built via parse_pr_review over reviews[] (degraded placeholder on malformed)
100:  // detail.checks built via parse_pr_check over statusCheckRollup[] (degraded placeholder)
101:  // detail.external_url = url field; review_decision + checks_status aggregates parsed

102: FUNCTION list_pr_comments(client, owner, name, number, cursor, page_size) -> Result<CommentsResponse>
102a:  // NEW PR-SPECIFIC method (src/github/mod.rs). It MUST query the GraphQL
102b:  //   repository.pullRequest(number:).comments connection — NOT repository.issue(number:).
102c:  //   GROUNDED (P00A §2d, verified live): for a PR NUMBER, `repository.issue(number: N)` is
102d:  //   NULL (NOT_FOUND), so reusing the existing issue `list_comments` (which queries
102e:  //   `repository.issue(number:).comments`, src/github/mod.rs L211-268) would SILENTLY return
102f:  //   ZERO comments for every PR — a silent-empty regression. `repository.pullRequest(number: N)
102g:  //   { comments(...) }` returns the PR comment timeline with the IDENTICAL node + pageInfo
102h:  //   shape, so the EXISTING comment node parsing and pagination semantics are reused unchanged.
102i:  query_with_after  = "query($owner: String!, $repo: String!, $number: Int!, $first: Int!, \
102j:                        $after: String) { repository(owner: $owner, name: $repo) { \
102k:                          pullRequest(number: $number) { comments(first: $first, after: $after) { \
102l:                            nodes { id databaseId author { login } createdAt lastEditedAt body } \
102m:                            pageInfo { hasNextPage endCursor } totalCount } } } }"
102n:  query_first_page  = same as above WITHOUT the `$after`/`after:` clauses
102o:  args = ["api","graphql","-f", "query=" + (cursor ? query_with_after : query_first_page),
102p:          "-F", "owner=" + owner, "-F", "repo=" + name, "-F", "number=" + number,
102q:          "-F", "first=" + page_size]
102r:  IF cursor present THEN args += ["-F", "after=" + cursor]
102s:  stdout = run_gh(args)?
103:   // REUSE the EXISTING comment node parser + page-info helper VERBATIM (the node shape is
104:   //   identical to the issue path): `parse_comments_json` (src/github/parse.rs) returns
105:   //   (comments oldest->newest, end_cursor, has_more) and `parse_page_info` extracts the cursor.
106:   //   The IssueComment domain type is reused for PR comments (PRs reuse IssueComment).
107:   (comments, end_cursor, has_more) = parse_comments_json(stdout)?
107a:  RETURN Ok(CommentsResponse{ comments, cursor: end_cursor, has_more })

108: FUNCTION create_pr_comment(client, owner, name, number, body) -> Result<IssueComment>
109:   // PR comments use the ISSUE comment REST endpoint
110:   args = ["api", "--method", "POST",
111:           "/repos/" + owner + "/" + name + "/issues/" + number + "/comments",
112:           "-f", "body=" + body]
113:   stdout = run_gh(args)?
114:   RETURN parse_created_comment_json(stdout)        // reuse existing parser

115: FUNCTION open_pull_request_in_browser(client, owner, name, number) -> Result<(), GhError>
116:   // REQ-PR-012 browser handoff for deferred merge/approve/review-submit. Reuses the gh
117:   // transport — `gh pr view --web` opens the default browser cross-platform, so NO bespoke
118:   // OS opener is introduced (none exists in src today; runtime/attach.rs only spawns
119:   // pbcopy/xclip, and runtime/preflight.rs uses an "open -a Docker" hint string).
120:   args = ["pr", "view", number, "--repo", owner + "/" + name, "--web"]
121:   run_gh(args)?                                     // non-zero exit -> categorize_error
122:   RETURN Ok(())                                     // no stdout payload to parse

123: // PrSendPayload = NEW struct in src/github/mod.rs mirroring SendPayload (mod.rs:78-89): structured,
124: //   owned fields ONLY. NO prompt_markdown/work_dir/signature (work_dir+signature come from the AGENT
125: //   via PrSendInfo like IssueSendInfo; markdown is rendered later by format_pr_prompt, c003 L176-187).
126: //   Fields: repository:String, pr_number:u64, pr_title:String, pr_body:String, pr_state:String,
127: //   head_ref:String, base_ref:String, external_url:String, review_summary:Vec<String>,
128: //   check_summary:Vec<String>, focused_comment:Option<String>, focused_comment_author:Option<String>,
129: //   pr_base_prompt:String.
130: FUNCTION build_pr_send_payload(repo_slug, pr_detail, focused_comment, pr_base_prompt) -> PrSendPayload
131:   // pure assembly; no I/O — mirrors GhClient::build_send_payload (mod.rs L432-455)
132:   state_str = map pr_detail.state -> "open" | "closed" | "merged"
133:   review_summary = summarize_reviews(pr_detail.reviews)   // Vec<String>, display-only
134:   check_summary  = summarize_checks(pr_detail.checks)     // Vec<String>, display-only
135:   RETURN PrSendPayload{ repository: repo_slug, pr_number, pr_title, pr_body, pr_state: state_str,
136:     head_ref, base_ref, external_url, review_summary, check_summary,
136a:    focused_comment: focused_comment.map(|c| c.body), focused_comment_author:
136b:    focused_comment.map(|c| c.author_login), pr_base_prompt }

137: // ---- parse helpers (src/github/parse_pr.rs) ----

138: FUNCTION parse_pull_requests_json(stdout) -> Result<{items, next_cursor, has_more}>
139:   json = serde_json::from_str(stdout) MAP Err -> ParseError
140:   data = json.data.search                          // GraphQL search envelope (mirror parse.rs L74-99)
141:   FOR each pr in data.nodes:
142:     number = pr.number; title = pr.title
143:     state = parse_pr_state(pr.state, pr.mergedAt)  // open/closed/merged (Finding 3: state enum
144:                                                    //   MERGED OR non-null mergedAt -> Merged)
145:     author_login = pr.author.login (or "ghost" if null)  // never silent drop -> log if null
146:     updated_at = pr.updatedAt
147:     head_ref = pr.headRefName; base_ref = pr.baseRefName
148:     is_draft = pr.isDraft
149:     review_decision = parse_review_decision(pr.reviewDecision)
150:     checks_status = parse_checks_rollup(rollup_nodes(pr.statusCheckRollup))  // see L181
151:     assignee_summary = join(pr.assignees.nodes[].login)
152:     labels_summary = join(pr.labels.nodes[].name)
153:     comment_count = pr.comments.totalCount
154:     PUSH PullRequest{...}
155:   (next_cursor, has_more) = parse_page_info(data.pageInfo)   // REUSE parse.rs::parse_page_info L424-433
156:   RETURN Ok({items, next_cursor, has_more})        // cursor + has_more are REAL endCursor/hasNextPage

157: FUNCTION parse_pull_request_detail_json(stdout, owner_name) -> Result<PullRequestDetail>
158:   json = serde_json::from_str(stdout) MAP Err -> ParseError
159:   state = parse_pr_state(json.state, json.mergedAt)   // Finding 3
160:   reviews = MAP json.reviews -> parse_pr_review     // NEVER drops; malformed -> degraded placeholder
161:   checks  = MAP rollup_nodes(json.statusCheckRollup) -> parse_pr_check  // NEVER drops; degraded placeholder
162:   review_decision = parse_review_decision(json.reviewDecision)
163:   checks_status = parse_checks_rollup(rollup_nodes(json.statusCheckRollup))
164:   RETURN Ok(PullRequestDetail{ repo_owner_name: owner_name, state, ..fields.., external_url: json.url,
165:            reviews, checks, comments: [], has_more_comments: false, comments_cursor: None })
166:            // comments/cursor/has_more_comments are filled by the separate list_pr_comments call (L93-97)

167: FUNCTION rollup_nodes(rollup_json) -> Vec<node>   // Finding 8 — normalize BOTH transport shapes
168:   // `gh pr view --json statusCheckRollup` -> rollup_json is a FLAT ARRAY of entries.
169:   // `gh api graphql search ... statusCheckRollup { contexts { nodes } }` -> the connection form.
170:   IF rollup_json is an array        -> RETURN rollup_json                 // pr-view flat shape
171:   IF rollup_json.contexts.nodes set -> RETURN rollup_json.contexts.nodes  // graphql connection shape
172:   IF rollup_json is null/absent     -> RETURN []                          // none yet (not an error)
173:   ELSE                              -> RETURN []                          // unknown shape -> empty (logged)

174: FUNCTION parse_pr_review(node) -> PrReview                // total function — never returns None
175:   // No silent drop (REQ-PR-013): a malformed/missing field yields a DISPLAYABLE degraded record,
176:   // not a discarded one. Counts and rendering still include it.
177:   author = node.author.login OR (LOG warn; "(unknown reviewer)")
178:   state  = parse_review_state(node.state) OR Commented       // default keeps it displayable
179:   submitted_at = node.submittedAt OR ""
180:   RETURN PrReview{ author_login: author, state, submitted_at, body: nonempty(node.body) }

181: FUNCTION parse_pr_check(node) -> PrCheck                   // total function — never returns None
182:   // No silent drop: malformed check still renders as a degraded "(unparseable check)" row.
183:   // Finding 8: handle BOTH CheckRun (name/status/conclusion/detailsUrl) and StatusContext
184:   // (context/state/targetUrl). Discriminate on __typename, fall back to field presence.
185:   __typename = node.__typename (may be absent)
186:   name = node.name OR node.context OR (LOG warn; "(unnamed check)")
187:   // raw status token precedence: conclusion (CheckRun terminal) -> state (StatusContext) ->
188:   //   status (CheckRun in-progress) -> "" ; mapped by parse_check_status (L195).
189:   raw_status = first_present(node.conclusion, node.state, node.status, "")
190:   status = parse_check_status(raw_status)               // unknown/empty -> Pending or Neutral (L195)
191:   conclusion_text = node.conclusion OR node.state OR node.status OR "unknown"   // raw display text
192:   url = node.detailsUrl OR node.targetUrl OR None
193:   RETURN PrCheck{ name, status, conclusion: conclusion_text, url }

194: FUNCTION sort_pull_requests(items) -> Vec<PullRequest>
195:   SORT BY updated_at DESC, THEN number ASC
196:   RETURN items

197: FUNCTION parse_pr_state(s, merged_at) -> PrState         // Finding 3
198:   // GraphQL PullRequest.state enum IS one of OPEN|CLOSED|MERGED directly; mergedAt is a backstop.
199:   IF s == "MERGED" OR merged_at present -> Merged
200:   ELSE IF s == "CLOSED" -> Closed
201:   ELSE -> Open

202: FUNCTION parse_review_decision(s) -> Option<PrReviewState>
203:   MATCH s: "APPROVED"->Approved, "CHANGES_REQUESTED"->ChangesRequested,
204:            "REVIEW_REQUIRED"->ReviewRequired, ""/null->None, _ -> Commented

205: FUNCTION parse_check_status(raw_status) -> PrCheckStatus  // Finding 8 — union of both enums
206:   // CheckRun.conclusion: SUCCESS|FAILURE|NEUTRAL|SKIPPED|CANCELLED|TIMED_OUT|ACTION_REQUIRED|
207:   //                      STALE|STARTUP_FAILURE ; CheckRun.status: QUEUED|IN_PROGRESS|COMPLETED|
208:   //                      WAITING|PENDING|REQUESTED ; StatusContext.state: EXPECTED|ERROR|FAILURE|
209:   //                      PENDING|SUCCESS
210:   MATCH uppercase(raw_status):
211:     "SUCCESS"                                   -> Success
212:     "FAILURE"|"ERROR"|"TIMED_OUT"|"STARTUP_FAILURE"|"ACTION_REQUIRED" -> Failure
213:     "NEUTRAL"|"SKIPPED"|"CANCELLED"|"STALE"     -> Neutral
214:     "PENDING"|"EXPECTED"|"QUEUED"|"IN_PROGRESS"|"WAITING"|"REQUESTED"|"" -> Pending
215:     _                                           -> Neutral   // unknown -> displayable, not dropped

216: FUNCTION parse_checks_rollup(nodes) -> PrCheckStatus      // Finding 8 — aggregate over both shapes
217:   IF nodes empty -> None
218:   per_node_status = nodes.map(|n| parse_check_status(first_present(n.conclusion, n.state, n.status, "")))
219:   IF any == Failure -> Failure
220:   IF any == Pending -> Pending
221:   IF all == Success -> Success
222:   ELSE -> Neutral

223: FUNCTION categorize_error(code, stderr) -> GhError   // REUSE existing
224:   IF stderr mentions auth -> NotAuthenticated
225:   IF stderr mentions rate limit -> RateLimited
226:   IF code == 4 OR stderr mentions "not found"/"forbidden" -> AccessDenied
227:   ELSE -> ApiError(stderr)
```

## Pagination & Comments Transport (grounded in current `src/`)

- **PR list pagination = REAL GraphQL cursor pagination.** `build_pr_search_args` / `list_pull_requests`
  mirror the ACTIVE issues list path: `src/github/parse.rs::build_issue_search_args` (L594-621) +
  `src/github/mod.rs::list_issues` (L134-159), which call `gh api graphql` with
  `search(type: ISSUE, query, first, after)` and read `pageInfo { hasNextPage endCursor }`.
  `endCursor`/`hasNextPage` are the canonical cursor + has-more (extracted by the REUSED
  `parse_page_info`, parse.rs L424-433). There is NO invented updatedAt/number cursor and NO
  `gh pr list --limit` window heuristic. (GitHub's `search(type: ISSUE, ...)` includes PRs; the
  `is:pr` qualifier narrows it to pull requests, and `... on PullRequest { ... }` selects PR fields.)
- **PR draft filtering = SERVER-SIDE search qualifier (Finding 3).** `build_pr_search_query` maps
  `filter.is_draft` to the `draft:true` / `draft:false` qualifier INSIDE the `search(query:...)`
  string (Some(true)->`draft:true`, Some(false)->`draft:false`, None->omit). This is verified server-
  side (P00A §2b: against cli/cli, `draft:true` returns only `isDraft=true` and `draft:false` only
  `isDraft=false`), so it composes with the SAME `endCursor`/`hasNextPage` cursor pagination as every
  other qualifier — there is NO client-side post-filter that would drop rows from a page and corrupt
  has-more/cursor semantics. `is:draft` is a one-sided alias (no negation), so the symmetric
  `draft:true`/`draft:false` pair is used. P07 `test_build_pr_search_query_emits_draft_qualifier`
  asserts the exact emitted token; P00A §2b is the gate that re-verifies the live qualifier before P06.
- **PR review/checks SIGNAL filtering = SERVER-SIDE search qualifiers (Finding 1; issue #20
  "review/workflow signals").** `build_pr_search_query` maps `filter.review_decision` to
  `review:approved` / `review:changes_requested` / `review:required` / `review:none` (Any -> omit)
  and `filter.checks_status` to `status:success` / `status:failure` / `status:pending` (Any -> omit),
  INSIDE the same `search(query:...)` string. These are verified server-side (P00A §2c), so they
  compose with the SAME `endCursor`/`hasNextPage` cursor pagination as every other qualifier — NO
  client-side post-filter that would drop rows from a page and corrupt has-more/cursor semantics.
  Qualifier emission order is deterministic (state, labels, author, assignee, reviewer, draft,
  review, checks, then free-text query). P07
  `test_build_pr_search_query_emits_review_and_checks_qualifiers` asserts the exact emitted tokens
  AND that the `first`/`after` cursor args are unchanged (pagination-safe, server-side); P00A §2c is
  the gate that re-verifies the live qualifiers before P06.
- **PR state mapping (Finding 3).** The list fragment and the `gh pr view --json` set BOTH request
  `state` (the GraphQL `PullRequestState` enum = OPEN|CLOSED|MERGED) AND `mergedAt`. `parse_pr_state`
  maps to `PrState::Merged` when `state == "MERGED"` OR `mergedAt` is non-null, to `Closed` when
  `state == "CLOSED"`, else `Open`. Tests assert a merged PR (state=MERGED, non-null mergedAt) maps to
  `Merged`, a closed-not-merged PR maps to `Closed`, and an open PR maps to `Open`.
- **statusCheckRollup parsing (Finding 8).** `rollup_nodes` normalizes the two transports
  (flat array from `gh pr view`, `contexts.nodes` connection from `gh api graphql`). `parse_pr_check`
  then handles BOTH entry shapes (CheckRun: `name`/`status`/`conclusion`/`detailsUrl`; StatusContext:
  `context`/`state`/`targetUrl`) by discriminating on `__typename` with field-presence fallback, and
  `parse_check_status` maps the union of CheckRun-conclusion, CheckRun-status, and StatusContext-state
  tokens into `PrCheckStatus`. No entry is ever dropped. Fixture-driven tests (P07) use REAL captured
  JSON (from the P00A capture step) containing BOTH a CheckRun and a StatusContext entry, asserting
  each maps to the expected `PrCheck` and that the rollup aggregate is correct.
- **PR detail comments = SEPARATE paginated `list_pr_comments` call (PR-specific GraphQL path).**
  `get_pull_request_detail` fetches metadata via `gh pr view --json ...` (the PR `--json` set
  deliberately OMITS the `comments` field) and then calls the NEW `GhClient::list_pr_comments` for
  the first comment page. This follows the COMMENTS-SOURCING precedent of `get_issue_detail`, with
  one accurate distinction: `get_issue_detail`'s `--json` list DOES include `comments` (mod.rs L180)
  and parses it (L197), but then DELIBERATELY OVERWRITES `detail.comments`/`comments_cursor`/
  `has_more_comments` from a distinct paginated `list_comments` call (mod.rs L198-202). PR detail
  takes the same end result by skipping the embedded `comments` field entirely (it would only be
  overwritten) and sourcing comments solely from `list_pr_comments`.
  **CRITICAL — query object path (Finding 1, verified P00A §2d):** the existing issue
  `list_comments` (mod.rs L211-268) queries `repository.issue(number:).comments(first, after)`.
  For a PR NUMBER, `repository.issue(number: N)` resolves to NULL (NOT_FOUND), so reusing
  `list_comments` for PRs would SILENTLY return zero comments — a silent-empty regression. Therefore
  PR comments MUST be fetched via `list_pr_comments`, which queries
  `repository.pullRequest(number:).comments(first, after) { nodes{...} pageInfo{hasNextPage
  endCursor} totalCount }`. The comment NODE shape is identical to the issue path, so
  `list_pr_comments` REUSES the existing `parse_comments_json` / `parse_page_info` helpers and the
  `IssueComment` domain type unchanged, and returns `(comments, end_cursor, has_more)` with the same
  pagination semantics. `has_more_comments`/`comments_cursor` therefore carry a REAL GraphQL
  endCursor from the `pullRequest.comments` connection, not an assumed `gh pr view` shape.
- **PR comment CREATE = REST issue-comment endpoint (verified valid for PRs).** `create_pr_comment`
  posts via `gh api --method POST /repos/{owner}/{repo}/issues/{number}/comments -f body=...`. Unlike
  the GraphQL FETCH path, the REST `/issues/{number}/comments` endpoint DOES accept a PR number (PRs
  ARE issues for the REST comments API), so this CREATE path is correct and is kept; it reuses
  `parse_created_comment_json`. (Only the GraphQL per-PR comments FETCH needed the `repository.issue`
  → `repository.pullRequest` correction.)

## Error Mapping Summary

| Condition | GhError | UI surface |
|-----------|---------|------------|
| `gh` binary absent | `NotInstalled` | "Install GitHub CLI" |
| auth failure | `NotAuthenticated` | "Run: gh auth login" |
| rate limit | `RateLimited` | scoped error + retry |
| 404 / forbidden | `AccessDenied` | scoped error |
| network failure | `NetworkError` | scoped error + retry |
| malformed JSON | `ParseError` | scoped error (logged) |
| invalid repo slug | `ApiError` | scoped "configure repository (owner/name)" |
