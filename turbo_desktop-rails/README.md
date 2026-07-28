# turbo_desktop-rails

Server-side Rails integration for [Turbo Desktop](https://github.com/aguspe/turbo_desktop) — the Turbo Native pattern for desktop apps.

This gem gives your Rails app awareness of the Turbo Desktop shell, exactly like `turbo-rails` does for Turbo Native mobile apps.

## Installation

Add to your Gemfile:

```ruby
gem "turbo_desktop-rails"
```

Then run:

```bash
bundle install
rails generate turbo_desktop:install
```

## Usage

### Detection

The gem detects Turbo Desktop requests via the User-Agent header (`Turbo Desktop/0.0.1 (macOS; aarch64)`).

```ruby
# In controllers
if turbo_desktop_app?
  # Desktop-specific logic
end

turbo_desktop_platform  # => "macos", "windows", "linux", or nil
turbo_desktop_arch      # => "aarch64", "x86_64", or nil
```

### View Helpers

```erb
<%# Render only inside the desktop app %>
<% turbo_desktop_only do %>
  <button data-controller="sidebar">Toggle Sidebar</button>
<% end %>

<%# Render only for regular web browsers %>
<% turbo_web_only do %>
  <nav class="web-navbar">...</nav>
<% end %>

<%# Bridge component data attributes %>
<%= tag.button "Export PDF",
    **turbo_desktop_bridge("menu-item",
      title: "Export PDF",
      shortcut: "Cmd+E"
    ) %>
```

### Desktop-only templates

Requests from the desktop app are marked with a Rails variant, so a whole
template can be written for it rather than branching inside a shared one:

```
app/views/orders/show.html.erb           # everyone
app/views/orders/show.html+desktop.erb   # the desktop app
```

Layouts work the same way — `app/views/layouts/application.html+desktop.erb`.
Rails falls back to the plain template wherever no variant exists, so this costs
nothing until you add one.

The block helpers above are still the right tool for a button or a nav bar. Reach
for a variant when the whole page differs.

Rename it, or turn it off, in the initializer:

```ruby
TurboDesktop.configure do |config|
  config.variant = :desktop   # nil leaves variants alone
end
```

It is added to any variants you have already set rather than replacing them.

### Path Configuration

The gem mounts a path configuration endpoint at `/turbo-desktop/path-configuration.json`:

```ruby
# config/initializers/turbo_desktop.rb
TurboDesktop.configure do |config|
  config.path_configuration = {
    settings: { screenshots_enabled: true },
    rules: [
      { patterns: ["/"], properties: { presentation: "default" } },
      { patterns: ["/new$", "/edit$"], properties: { presentation: "modal" } },
      { patterns: ["/settings"], properties: { presentation: "native" } }
    ]
  }
end
```

## Requirements

- Ruby >= 3.1.0
- Rails >= 7.0
- turbo-rails >= 1.0

## License

MIT — see [LICENSE](LICENSE) for details.
