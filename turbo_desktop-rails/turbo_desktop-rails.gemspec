require_relative "lib/turbo_desktop/version"

Gem::Specification.new do |spec|
  spec.name          = "turbo_desktop-rails"
  spec.version       = TurboDesktop::VERSION
  spec.authors       = ["RaiderHQ"]
  spec.email         = ["hello@raiderhq.com"]

  spec.summary       = "Turbo Native for Desktop — Rails integration"
  spec.description   = "Server-side helpers for Turbo Desktop: User-Agent detection, view helpers, and path configuration endpoint for desktop apps built with Turbo/Hotwire."
  spec.homepage      = "https://github.com/aguspe/turbo_desktop"
  spec.license       = "MIT"

  spec.required_ruby_version = ">= 3.1.0"

  spec.metadata["homepage_uri"]      = spec.homepage
  spec.metadata["source_code_uri"]   = spec.homepage
  spec.metadata["changelog_uri"]     = "#{spec.homepage}/blob/main/turbo_desktop-rails/CHANGELOG.md"
  spec.metadata["rubygems_mfa_required"] = "true"

  spec.files = Dir["lib/**/*", "app/**/*", "config/**/*", "LICENSE", "README.md", "CHANGELOG.md"]
  spec.require_paths = ["lib"]

  spec.add_dependency "rails", ">= 7.0"
  spec.add_dependency "turbo-rails", ">= 1.0"
end
