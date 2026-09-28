module TurboDesktop
  class Configuration
    attr_accessor :path_configuration, :user_agent_pattern, :inspector_enabled,
                  :inspector_mount_path, :variant

    def initialize
      @path_configuration = default_path_configuration
      @user_agent_pattern = /Turbo Desktop/
      @inspector_enabled = false
      # Rails variant set on requests from the desktop app, so views can be
      # written as show.html+desktop.erb. Set to nil to leave variants alone.
      @variant = :desktop
      # Where the engine is mounted; the inspector meta tag advertises assets
      # under this prefix. Override if you mount the engine elsewhere.
      @inspector_mount_path = "/turbo-desktop"
    end

    def path_configuration_json
      @path_configuration.to_json
    end

    # How the shell will present a path: "default", "modal", "new_window" and
    # so on. Read the way the shell reads it: every rule is tried, and the
    # last one that matches decides.
    def presentation_for(path)
      rules = @path_configuration.to_h.with_indifferent_access[:rules]

      Array(rules).reduce("default") do |presentation, rule|
        next presentation unless Array(rule[:patterns]).any? { |pattern| matches?(pattern, path) }

        rule.dig(:properties, :presentation).presence&.to_s || "default"
      end
    end

    private

    # A pattern that is not one matches nothing, as in the shell.
    def matches?(pattern, path)
      Regexp.new(pattern.to_s).match?(path.to_s)
    rescue RegexpError
      false
    end

    def default_path_configuration
      {
        settings: {
          screenshots_enabled: false
        },
        rules: [
          { patterns: [ "/" ], properties: { presentation: "default" } }
        ]
      }
    end
  end
end
