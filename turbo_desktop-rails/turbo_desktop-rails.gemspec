require_relative "lib/turbo_desktop/version"

Gem::Specification.new do |spec|
  spec.name          = "turbo_desktop-rails"
  spec.version       = TurboDesktop::VERSION
  spec.authors       = [ "aguspe" ]
  spec.email         = [ "hello@raiderhq.com" ]

  spec.summary       = "Turbo Native for Desktop — Rails integration"
  spec.description   = "Server-side helpers for Turbo Desktop: User-Agent detection, view helpers, and path configuration endpoint for desktop apps built with Turbo/Hotwire."
  spec.homepage      = "https://github.com/aguspe/turbo_desktop"
  spec.license       = "MIT"

  # Rails 8's own floor. Anything higher and Bundler quietly resolves back to
  # 0.0.1 for people on 3.2 — a version predating the install generator — rather
  # than telling them the gem does not support their Ruby.
  spec.required_ruby_version = ">= 3.2.0"

  spec.metadata["homepage_uri"]      = spec.homepage
  spec.metadata["source_code_uri"]   = spec.homepage
  spec.metadata["changelog_uri"]     = "#{spec.homepage}/blob/main/turbo_desktop-rails/CHANGELOG.md"
  spec.metadata["rubygems_mfa_required"] = "true"

  spec.files = Dir["lib/**/*", "app/**/*", "config/**/*", "LICENSE", "README.md", "CHANGELOG.md"]
  spec.require_paths = [ "lib" ]

  spec.add_dependency "rails", ">= 7.0"
  spec.add_dependency "turbo-rails", ">= 1.0"
end
