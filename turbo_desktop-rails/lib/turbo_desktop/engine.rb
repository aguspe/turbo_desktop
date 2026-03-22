require "turbo_desktop/view_helpers"
require "turbo_desktop/detection"

module TurboDesktop
  class Engine < ::Rails::Engine
    isolate_namespace TurboDesktop

    config.to_prepare do
      ActionController::Base.include TurboDesktop::Detection unless ActionController::Base < TurboDesktop::Detection
      ActionView::Base.include TurboDesktop::ViewHelpers unless ActionView::Base < TurboDesktop::ViewHelpers
    end
  end
end
