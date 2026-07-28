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

      before_action :set_turbo_desktop_variant
    end

    # The variant list a request should end up with, or nil to leave it alone.
    #
    # Kept separate from the controller so the decision can be exercised on its
    # own. Adds to whatever variants are already set rather than replacing them:
    # an app may well be using variants for something else.
    def self.variant_for(current:, desktop:, configured:)
      return nil if configured.blank? || !desktop

      current = Array(current).map(&:to_sym)
      configured = configured.to_sym
      return nil if current.include?(configured)

      current + [ configured ]
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

    # Mark desktop requests with a Rails variant, so a whole template can be
    # written for the desktop app instead of branching inside a shared one:
    #
    #   app/views/orders/show.html.erb           # everyone
    #   app/views/orders/show.html+desktop.erb   # the desktop app
    #
    # Layouts pick it up too — layouts/application.html+desktop.erb. Rails falls
    # back to the plain template wherever no variant exists, so this costs
    # nothing until you add one.
    #
    # Set config.variant to nil to turn it off.
    def set_turbo_desktop_variant
      variant = TurboDesktop::Detection.variant_for(
        current: request.variant,
        desktop: turbo_desktop_app?,
        configured: TurboDesktop.configuration.variant
      )

      request.variant = variant if variant
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
