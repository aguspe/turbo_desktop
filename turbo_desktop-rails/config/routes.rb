TurboDesktop::Engine.routes.draw do
  get "path-configuration", to: "path_configurations#show", defaults: { format: :json }

  # Dev Inspector assets, served same-origin so the desktop shell can import() them.
  get "inspector.js", to: "inspector_assets#show", format: false
  get "inspector/*module", to: "inspector_assets#show", format: false
end
