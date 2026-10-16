# gitat

Git statistics for the changes between two branches or releases.

```
gitat diff <path> <from> <to> [--output web|json] [--config <file>]
gitat combine <file.json>... [--output web|json] [--config <file>]
```

- `web` (default) writes `<project>-<from>-<to>.html`
- `json` writes `<project>-<from>-<to>.json`, handy for combining results across repos
- `combine` merges several `diff --output json` exports into one umbrella report

## Config

Optional `--config gitat.toml`:

```toml
name = "GNOME"
hide = ["bus_factor", "code_decay"]
bots = ["damned-lies", "Translation Robot"]
```

- `name` — override the project name shown in the report / used in the output filename / JSON `project` field
- `bots` — extra name/email substrings (case-insensitive) marking bot accounts, on top of the built-in detection (`[bot]`, dependabot, renovate, marge-bot, bors, any name or email word `bot`/`robot`, …). Bots are left out of Top Contributors, Affiliations, the contributor count, Bus Factor and Highest Impact Commits, and counted under Automated Bots instead
- `hide` — chart keys to omit from the web report:

| key | chart |
| --- | --- |
| `heatmap` | Contribution Activity |
| `pulse` | Yearly Productivity Pulse |
| `commit_velocity` | Commit Velocity |
| `weekly_distribution` | Weekly Distribution |
| `hourly_activity` | Hourly Activity Profile |
| `contributors` | Top Contributors |
| `affiliations` | Affiliations |
| `churn_hotspots` | Churn Hotspots |
| `bus_factor` | Bus Factor Risk |
| `impact_commits` | Highest Impact Commits |
| `code_decay` | Code Decay |
