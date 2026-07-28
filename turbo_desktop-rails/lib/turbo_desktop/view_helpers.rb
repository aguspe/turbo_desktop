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

    # Attribute name fragments we are willing to build a data-* attribute from.
    # Values are escaped by the tag helpers, but names are interpolated, so they
    # are restricted rather than escaped.
    BRIDGE_NAME_PATTERN = /\A[a-zA-Z0-9_-]+\z/

    # Renders a bridge component data attribute.
    #
    #   <%= tag.button "Export", **turbo_desktop_bridge("menu-item", title: "Export PDF", shortcut: "Cmd+E") %>
    def turbo_desktop_bridge(component, **options)
      validate_bridge_name!(component, "component")

      attrs = { "data-turbo-desktop-bridge" => component.to_s }
      options.each do |key, value|
        validate_bridge_name!(key, "option name")
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

    # Returns true when the Dev Inspector is enabled in configuration.
    def turbo_desktop_inspector?
      TurboDesktop.configuration.inspector_enabled
    end

    # Emits the <meta> tag that enables the Dev Inspector in the browser, or nil
    # when the inspector is disabled. Place in your layout <head>; it is a no-op
    # in production unless you explicitly enable the inspector there.
    #
    # The tag also carries the same-origin URL of the inspector entry module
    # (served by this engine) so the desktop shell's turbo-desktop.js can
    # import() it instead of guessing a relative path.
    #
    #   <%= turbo_desktop_inspector_meta_tag %>
    def turbo_desktop_inspector_meta_tag
      return nil unless turbo_desktop_inspector?

      tag.meta(name: "turbo-desktop-inspector", content: "enabled",
               data: { inspector_url: turbo_desktop_inspector_url })
    end

    # Same-origin URL of the inspector entry module, under the engine's mount
    # path (configurable via config.inspector_mount_path).
    def turbo_desktop_inspector_url
      "#{TurboDesktop.configuration.inspector_mount_path.chomp("/")}/inspector.js"
    end

    private

    def validate_bridge_name!(name, label)
      return if name.to_s.match?(BRIDGE_NAME_PATTERN)

      raise ArgumentError,
            "turbo_desktop_bridge #{label} #{name.inspect} may only contain " \
            "letters, numbers, hyphens and underscores"
    end
  end
end
