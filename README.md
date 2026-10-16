# gitat

Git statistics for the changes between two branches or releases.

```
gitat diff <path> <from> <to> [--branch <ref>] [--output web|json] [--config <file>]
gitat combine <file.json>... [--output web|json] [--config <file>]
```

- `<from>` / `<to>` are refs (branch, tag, commit) or dates (`2024-01-01`, `2024-01-01 12:00:00 +0200`, `6 months ago`, ...). A date picks the first commit on `--branch` (default `HEAD`, first-parent history) at or after it; commits on the `<from>` date itself are included. A `<to>` date past the latest commit means the whole branch
- `web` (default) writes `<project>-<from>-<to>.html`
- `json` writes `<project>-<from>-<to>.json`, handy for combining results across repos
- `combine` merges several `diff --output json` exports into one umbrella report

## Config

Optional `--config gitat.toml`:

```toml
name = "GNOME"
hide = ["bus_factor", "code_decay"]
bots = ["damned-lies", "Translation Robot"]

[affiliations]
"mclasen@gmail.com" = "Red Hat"   # a specific address
"gnome.org" = "GNOME Foundation"  # a whole domain
"example.com" = ""                # treat as personal / unaffiliated
```

- `name` — override the project name shown in the report / used in the output filename / JSON `project` field
- `bots` — extra name/email substrings (case-insensitive) marking bot accounts, on top of the built-in detection (`[bot]`, dependabot, renovate, marge-bot, bors, any name or email word `bot`/`robot`, …). Bots are left out of Top Contributors, Affiliations, the contributor count, Bus Factor and Highest Impact Commits, and counted under Automated Bots instead
- `affiliations` — company per email address or email domain (case-insensitive) for the Affiliations chart, overriding the built-in domain table. An address match wins over a domain match; an empty company marks it as personal. Personal webmail domains (gmail.com, googlemail.com, …) aren't counted as a company unless mapped here
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
