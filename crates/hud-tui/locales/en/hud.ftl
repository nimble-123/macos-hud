tab-overview = Overview
tab-workspaces = Workspaces
tab-updates = Updates
tab-ai = AI
tab-log = Log
tab-settings = Settings
key-tab = tab
key-quit = q
hint-switch = switch
hint-quit = quit
key-arrows = ↑↓
key-f = f
key-r = r
hint-select = select
hint-fast-forward = fast-forward
hint-refresh = refresh
hint-check = check
hint-reload-log = reload
syncing = Fetching and syncing…
ws-none = No workspaces configured yet.
ws-unavailable = not available
ws-auto-off = automatic fast-forward off
ws-empty = No repositories found.
branch-detached = (detached)
state-up-to-date = up to date
state-behind = { $count ->
    [one] 1 commit behind, fast-forward possible
   *[other] { $count } commits behind, fast-forward possible
}
state-fast-forwarded = { $count ->
    [one] fast-forwarded 1 commit
   *[other] fast-forwarded { $count } commits
}
state-error = error: { $message }
skip-operation = { $operation } in progress
skip-detached = detached HEAD
skip-no-commits = no commits yet
skip-no-upstream = no upstream
skip-tracked = local changes
skip-diverged = diverged
skip-ahead = { $count ->
    [one] 1 local commit not pushed
   *[other] { $count } local commits not pushed
}
skip-opted-out = behind, automatic fast-forward off (f)
op-merge = merge
op-rebase = rebase
op-cherry-pick = cherry-pick
op-revert = revert
op-bisect = bisect
setup-title = Welcome to hud
setup-step-workspaces = Step 1 of 2: workspaces
setup-workspaces-help = Folders in which hud looks for Git repositories. Space selects a suggestion; type your own path below.
setup-add-path = Add path:
setup-path-invalid = A path must start with / or ~/.
setup-need-workspace = Select at least one workspace.
setup-step-icons = Step 2 of 2: symbols
setup-icons-help = Which line shows every symbol? Nerd icons need a Nerd Font in your terminal.
setup-icons-unicode = Unicode (recommended)
setup-icons-nerd = Nerd Font
setup-icons-ascii = ASCII
key-space = space
key-enter = enter
key-esc = esc
hint-toggle = select
hint-next = next
hint-finish = finish
hint-cancel = cancel
hint-move = move
scan-local = Local scan
scan-sync = Fetch and sync
ws-scanning = Looking for repositories …
src-homebrew = Homebrew
src-npm = npm (global)
src-skills = Skills
src-missing = not found
src-missing-hint = { $program } not found in the login shell PATH. Install it or set `[tools] { $key } = "…"` in the config.
src-broken = error: { $message }
src-checking = checking …
src-up-to-date = everything up to date
src-skills-installed = { $count ->
    [one] 1 skill installed, updates unknown
   *[other] { $count } skills installed, updates unknown
}
src-update-all = Update all skills
bump-major = major update
pinned = pinned
job-queued = waiting
job-running = running
job-succeeded = installed
job-needs-interaction = needs a password, run it in a terminal
job-denied = not allowed
job-failed = failed: { $message }
job-timed-out = timed out
key-u = u
hint-install = install
confirm-title = Install update?
confirm-major = Major update: it may contain incompatible changes.
confirm-install = install
confirm-cancel = cancel
jobs-running = { $count ->
    [one] 1 installation running
   *[other] { $count } installations running
}
settings-loading = Loading settings …
settings-display = Display
settings-language = Language
settings-theme = Theme
settings-icons = Symbols
settings-workspaces = Workspaces
settings-auto-ff = Automatic fast-forward
settings-add-workspace = Add workspace
settings-auto-install = Automatic installation
settings-rejected = Not saved: { $reason }
settings-remove-title = Remove workspace?
settings-remove-body = Only from the config. Folders and repositories stay as they are.
pref-auto = Automatic
lang-de = German
lang-en = English
theme-dark = Dark
theme-light = Light
opt-brew-formulae = Homebrew formulae
opt-brew-casks = Homebrew casks
opt-npm = Global npm (never across major versions)
toggle-on = on
toggle-off = off
confirm-remove = remove
key-a = a
key-d = d
hint-change = change
hint-add = add
hint-remove = remove
hint-reload = reload config
log-filter = Filter:
log-filter-all = all
log-filter-auto = automatic
log-filter-manual = manual
log-filter-problems = problems
log-search = Search:
log-empty = No actions logged yet.
log-no-match = No action matches the filter and search.
log-col-time = Time
log-col-action = Action
log-col-trigger = Trigger
log-col-result = Result
log-col-target = Target
log-detail-target = Target
log-detail-output = Output end
log-duration = took { $duration }
kind-fetch = Fetch
kind-fast-forward = Fast-forward
kind-install = Install
trigger-auto = auto
trigger-manual = manual
outcome-running = running
outcome-succeeded = done
outcome-failed = failed
outcome-timed-out = timed out
outcome-skipped = skipped
outcome-cancelled = cancelled
key-slash = /
hint-details = details
hint-search = search
hint-filter = filter
log-detail-summary = Summary
job-cancelled = cancelled
quit-title = Quit hud?
quit-running = { $count ->
    [one] 1 installation is still running.
   *[other] { $count } installations are still running.
}
quit-cancel-warning = Cancelling can leave a package half installed.
quit-wait = wait, then quit
quit-cancel = cancel and quit
quit-back = back
quit-waiting = { $count ->
    [one] Waiting for 1 installation, then hud quits
   *[other] Waiting for { $count } installations, then hud quits
}
hint-quit-now = quit now
ai-loading = Reading AI usage …
ai-no-sources = No AI source enabled.
ai-claude = Claude Code
ai-codex = Codex
ai-prices-as-of = prices as of { $date }
ai-no-logs = No logs at { $path }
ai-read-failed = Logs not readable: { $message }
ai-unreadable = { $count ->
    [one] partial: 1 line not readable
   *[other] partial: { $count } lines not readable
}
ai-sessions = Sessions
ai-responses = Responses
ai-tokens = Tokens
ai-input = Input
ai-output = Output
ai-cache-write = Cache write
ai-cache-read = Cache read
ai-cost = Cost
ai-model = Model
ai-no-usage = No usage in this period.
ai-partial = partial
ai-api-equivalent = API equivalent
ai-estimate = estimate
period-today = Today
period-7-days = 7 days
period-30-days = 30 days
period-this-month = This month
period-last-month = Last month
key-left-right = ←→
hint-scroll = scroll
hint-period = period
telemetry = Telemetry
telemetry-off = off (config)
telemetry-port-busy = port { $port } not available: { $reason }
telemetry-enabled = receiving on 127.0.0.1:{ $port }, Claude Code sends
telemetry-disabled = Claude Code does not send yet
telemetry-enable = switch on
telemetry-conflict = own OTel settings in Claude Code ({ $keys }), hud changes nothing
telemetry-unreadable = settings.json not readable: { $reason }
telemetry-restart = Restart running Claude Code sessions so they send.
telemetry-dialog-title = Switch on telemetry in Claude Code?
telemetry-dialog-body = hud adds to "env" in ~/.claude/settings.json:
telemetry-dialog-note = Metrics only, no prompts or answers. Other entries stay as they are.
telemetry-dialog-write = add
key-t = t
ai-active-time = Active time
ai-active-user = input
ai-active-cli = Claude
ai-telemetry-note = only while hud runs
ai-code = Code
ai-lines = +{ $added } / −{ $removed } lines
ai-commits = { $count ->
    [one] 1 commit
   *[other] { $count } commits
}
ai-pull-requests = { $count ->
    [one] 1 PR
   *[other] { $count } PRs
}
ai-cost-claude = Cost per Claude
overview-measuring = Measuring system values …
overview-cpu = CPU
overview-memory = Memory
overview-processes = Processes
overview-repos-behind = { $count ->
    [one] 1 repo behind
   *[other] { $count } repos behind
}
overview-repos-skipped = { $count } skipped
overview-repos-diverged = { $count } diverged
overview-repos-errors = { $count ->
    [one] 1 error
   *[other] { $count } errors
}
overview-workspaces-unavailable = { $count ->
    [one] 1 workspace unavailable
   *[other] { $count } workspaces unavailable
}
overview-updates = { $count ->
    [one] 1 update
   *[other] { $count } updates
}
overview-calm = All up to date.
overview-calm-since = All up to date. Last check { $minutes } min ago.
overview-checking = First check running …
overview-ai-today = AI today
overview-calm-now = All up to date. Checked just now.
ai-files-read = { $files } log files in { $ms } ms
kpi-today = today
kpi-7-days = 7 d
kpi-30-days = 30 d
kpi-output-today = output today
kpi-cache-hit = Cache hit
kpi-models = Models
kpi-effort = Effort
kpi-subagents = Subagents
kpi-of-fresh = of fresh tokens
kpi-thinking = thinking
kpi-of-output = of output
kpi-activity = Activity
kpi-sessions = { $count ->
    [one] 1 session
   *[other] { $count } sessions
}
kpi-sessions-30 = { $count } sessions in 30 d
kpi-responses = { $count ->
    [one] 1 response
   *[other] { $count } responses
}
kpi-tool-calls = { $count ->
    [one] 1 tool call
   *[other] { $count } tool calls
}
kpi-context = Context
kpi-projects = Projects
kpi-just-now = just now
kpi-minutes-ago = { $count } min ago
kpi-hours-ago = { $count } h ago
kpi-days-ago = { $count } d ago
ai-token-kinds = Token kinds
kpi-consumption = Consumption
kpi-today-label = Today
kpi-details = Details
kpi-now = current
kpi-totals = Total
kpi-axis-midnight = 0:00
kpi-axis-day-end = 24:00
kpi-axis-month-start = start of month
kpi-axis-month-end = end of month
calendar-title = AI activity
calendar-weeks = last { $count } weeks
calendar-mon = Mon
calendar-wed = Wed
calendar-fri = Fri
calendar-summary = { $answers } answers, { $days } active days, longest streak { $streak } days
calendar-less = less
calendar-more = more
calendar-git-title = Git activity
calendar-git-summary = { $answers } commits, { $days } active days, longest streak { $streak } days
help-title = Keys
help-everywhere = Everywhere
help-close = Any key closes the help.
help-key-fast-forward = f, Enter
help-key-install = u, Enter
help-key-scroll = ↑↓, PgUp/PgDn, Home, End
help-key-period = ←→, 1–5
help-key-change = Enter, Space, ←→
help-key-tabs = Tab, Shift-Tab
help-key-help = ?
help-key-ctrl-c = Ctrl-C
help-select = move the selection, wrapping at both ends
help-details = enlarge or shrink the details
help-fast-forward = fast-forward the selected repo
help-sync = start fetch and sync now
help-install = install the selected package (asks first)
help-check = check update sources now
help-scroll = scroll the tab when it does not fit
help-period = switch the period for the whole tab
help-telemetry = switch on telemetry in Claude Code (asks first)
help-search = search, filters while typing
help-filter = cycle the filter: all, automatic, manual, problems
help-clear-search = drop the search
help-reload-log = reload the log
help-change = change the value or flip the switch
help-add-workspace = add a workspace
help-remove-workspace = remove a workspace (asks first)
help-reload-config = reload the config file
help-tabs = next or previous tab
help-help = this help
help-quit = quit, asks while an installation runs
help-quit-now = quit, even from an input
hint-help = help
ai-reasoning = Reasoning
kpi-reasoning = Reasoning
kpi-plan = Plan usage
kpi-plan-note = { $plan } plan, as of { $ago }
kpi-plan-as-of = as of { $ago }
plan-window-minutes = { $count } min
plan-window-hours = { $count ->
    [one] 1 hour
   *[other] { $count } hours
}
plan-window-days = { $count ->
    [one] 1 day
   *[other] { $count } days
}
plan-window-primary = main window
plan-window-secondary = second window
plan-resets = resets { $time }
plan-reset-done = reset at { $time }, new value after the next use
