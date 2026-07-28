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

    private

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
