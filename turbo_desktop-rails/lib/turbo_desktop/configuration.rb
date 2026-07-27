module TurboDesktop
  class Configuration
    attr_accessor :path_configuration, :user_agent_pattern, :inspector_enabled

    def initialize
      @path_configuration = default_path_configuration
      @user_agent_pattern = /Turbo Desktop/
      @inspector_enabled = false
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
