module TurboDesktop
  class PathConfigurationsController < ActionController::Base
    # GET /turbo-desktop/path-configuration.json
    #
    # Returns the path configuration JSON that the desktop app uses
    # to determine how to present each URL (default, modal, new window, native).
    #
    # This endpoint mirrors the pattern used by Hotwire Native mobile apps.
    def show
      render json: TurboDesktop.configuration.path_configuration
    end
  end
end
