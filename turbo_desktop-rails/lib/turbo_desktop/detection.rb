module TurboDesktop
  # Detects whether the current request is coming from a Turbo Desktop app
  # by inspecting the User-Agent string.
  #
  # The Turbo Desktop shell sets a User-Agent like:
  #   "Turbo Desktop/0.1.0 (macOS; aarch64)"
  #
  # This mirrors how turbo-rails detects Turbo Native mobile apps.
  module Detection
    extend ActiveSupport::Concern

    included do
      helper_method :turbo_desktop_app?, :turbo_desktop_platform, :turbo_desktop_arch
    end

    # Returns true if the request is from a Turbo Desktop app.
    def turbo_desktop_app?
      request.user_agent.to_s.match?(TurboDesktop.configuration.user_agent_pattern)
    end

    # Returns the desktop platform: "macos", "windows", "linux", or nil.
    def turbo_desktop_platform
      return nil unless turbo_desktop_app?

      ua = request.user_agent.to_s
      case ua
      when /macOS/i then "macos"
      when /Windows/i then "windows"
      when /Linux/i then "linux"
      else nil
      end
    end

    # Returns the architecture: "aarch64", "x86_64", or nil.
    def turbo_desktop_arch
      return nil unless turbo_desktop_app?

      ua = request.user_agent.to_s
      case ua
      when /aarch64/i then "aarch64"
      when /x86_64/i then "x86_64"
      else nil
      end
    end
  end
end
