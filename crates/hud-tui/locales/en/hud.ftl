tab-overview = Overview
tab-workspaces = Workspaces
tab-updates = Updates
tab-ai = AI
tab-log = Log
tab-settings = Settings
tab-placeholder = { $tab } is not built yet.
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
hint-period = period
