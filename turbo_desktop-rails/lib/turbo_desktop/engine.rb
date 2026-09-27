require "turbo_desktop/view_helpers"
require "turbo_desktop/detection"

module TurboDesktop
  class Engine < ::Rails::Engine
    isolate_namespace TurboDesktop

    config.to_prepare do
      ActionController::Base.include TurboDesktop::Detection unless ActionController::Base < TurboDesktop::Detection
      ActionView::Base.include TurboDesktop::ViewHelpers unless ActionView::Base < TurboDesktop::ViewHelpers
    end

    # The shell writes one line of handshake to our stdin before anything
    # else. Read it at boot so `TurboDesktop::Native` works everywhere in the
    # app, including from a background job with no page open.
    #
    # Exactly one line is consumed: the shell keeps the pipe open afterwards
    # so that closing it is the signal to exit, which is the only thing that
    # survives the shell being force-quit.
    initializer "turbo_desktop.native_handshake" do
      TurboDesktop::Native.read_handshake!
    rescue StandardError => e
      Rails.logger&.warn("[turbo_desktop] no native handshake: #{e.class}: #{e.message}")
    end
  end
end
