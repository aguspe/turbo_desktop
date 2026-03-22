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
