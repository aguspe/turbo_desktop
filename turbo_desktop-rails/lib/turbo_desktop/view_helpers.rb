module TurboDesktop
  module ViewHelpers
    # Returns true if the request is from a Turbo Desktop app.
    def turbo_desktop_app?
      request.user_agent.to_s.match?(TurboDesktop.configuration.user_agent_pattern)
    end

    # Returns the desktop platform: "macos", "windows", "linux", or nil.
    def turbo_desktop_platform
      return nil unless turbo_desktop_app?

      case request.user_agent.to_s
      when /macOS/i then "macos"
      when /Windows/i then "windows"
      when /Linux/i then "linux"
      end
    end

    # Returns the architecture: "aarch64", "x86_64", or nil.
    def turbo_desktop_arch
      return nil unless turbo_desktop_app?

      case request.user_agent.to_s
      when /aarch64/i then "aarch64"
      when /x86_64/i then "x86_64"
      end
    end

    # Renders a bridge component data attribute.
    #
    #   <%= tag.button "Export", **turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E") %>
    def turbo_desktop_bridge(component, **options)
      attrs = { "data-turbo-desktop-bridge" => component }
      options.each do |key, value|
        attrs["data-turbo-desktop-bridge-#{key}"] = value.to_s
      end
      attrs
    end

    # Conditionally render content only for desktop apps.
    def turbo_desktop_only(&block)
      capture(&block) if turbo_desktop_app?
    end

    # Conditionally render content only for web (non-desktop) users.
    def turbo_web_only(&block)
      capture(&block) unless turbo_desktop_app?
    end
  end
end
