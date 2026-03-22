require "turbo_desktop/version"
require "turbo_desktop/engine" if defined?(Rails)
require "turbo_desktop/configuration"
require "turbo_desktop/detection"

module TurboDesktop
  class << self
    def configuration
      @configuration ||= Configuration.new
    end

    def configure
      yield(configuration)
    end

    def reset_configuration!
      @configuration = Configuration.new
    end
  end
end
