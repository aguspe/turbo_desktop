module TurboDesktop
  # Serves the Dev Inspector's JavaScript from the Rails origin so the desktop
  # shell's `turbo-desktop.js` can `import()` it same-origin.
  #
  # The shell points its WebView at your Rails app (server_url), so a relative
  # `import("./inspector.js")` resolves against the Rails origin — which is why
  # these assets must be served here, not from the bundled Tauri frontend.
  #
  # Only the fixed set of inspector modules is served (strict allow-list, so no
  # path traversal). Enabled implicitly by mounting the engine; the meta-tag
  # helper points the shell at these URLs.
  class InspectorAssetsController < ActionController::Base
    ASSET_ROOT = TurboDesktop::Engine.root.join("lib/turbo_desktop/inspector_assets").freeze

    # Relative paths (as requested by the browser) → allowed. Anything else 404s.
    ALLOWED = %w[
      inspector.js
      inspector/state.js
      inspector/panel.js
      inspector/bridge-tap.js
      inspector/catalog.js
    ].freeze

    # GET /turbo-desktop/inspector.js
    # GET /turbo-desktop/inspector/<module>.js
    def show
      rel = requested_asset
      return head(:not_found) unless ALLOWED.include?(rel)

      path = ASSET_ROOT.join(rel)
      return head(:not_found) unless File.file?(path)

      response.set_header("Cache-Control", "public, max-age=3600")
      render body: File.read(path), content_type: "text/javascript"
    end

    private

    def requested_asset
      mod = params[:module]
      mod.blank? ? "inspector.js" : "inspector/#{mod}"
    end
  end
end
