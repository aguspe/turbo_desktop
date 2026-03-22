Gem::Specification.new do |spec|
  spec.name          = "turbo_desktop-rails"
  spec.version       = "0.1.0"
  spec.authors       = ["RaiderHQ"]
  spec.email         = ["hello@raiderhq.com"]

  spec.summary       = "Turbo Native for Desktop — Rails integration"
  spec.description   = "Server-side helpers for Turbo Desktop: User-Agent detection, view helpers, and path configuration endpoint for desktop apps built with Turbo/Hotwire."
  spec.homepage      = "https://github.com/RaiderHQ/turbo_desktop"
  spec.license       = "MIT"

  spec.required_ruby_version = ">= 3.1.0"

  spec.files = Dir["lib/**/*", "app/**/*", "config/**/*", "LICENSE", "README.md"]
  spec.require_paths = ["lib"]

  spec.add_dependency "rails", ">= 7.0"
  spec.add_dependency "turbo-rails", ">= 1.0"
end
