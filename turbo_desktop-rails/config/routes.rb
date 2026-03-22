TurboDesktop::Engine.routes.draw do
  get "path-configuration", to: "path_configurations#show", defaults: { format: :json }
end
