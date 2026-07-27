require "bundler/setup"
require "minitest/autorun"
require "active_support"
require "active_support/core_ext"
require "action_controller"
require "action_view"
require "action_dispatch"

# Load the gem
require "turbo_desktop"
require "turbo_desktop/view_helpers"
require "turbo_desktop/detection"
require "turbo_desktop/configuration"

# A single shared Rails app + mounted engine for integration tests. Booting
# more than one Rails::Application in a process is not allowed, so any test that
# needs routes (path-configuration, inspector assets) uses this one.
require "rails"
require "turbo_desktop/engine"

class DummyApp < Rails::Application
  config.eager_load = false
  config.secret_key_base = "test-secret-key-base-for-turbo-desktop-tests"
  config.hosts.clear
end

Rails.application.initialize! unless Rails.application.initialized?
Rails.application.routes.draw do
  mount TurboDesktop::Engine => "/turbo-desktop"
end

# Reset configuration between tests
module ConfigurationReset
  def setup
    super
    TurboDesktop.reset_configuration!
  end
end

Minitest::Test.prepend(ConfigurationReset)

# Stub request object for controller/view helper tests
class StubRequest
  attr_accessor :user_agent

  def initialize(user_agent = "")
    @user_agent = user_agent
  end
end
