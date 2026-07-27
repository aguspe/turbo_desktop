require "test_helper"

class ConfigurationTest < Minitest::Test
  # --- Default values ---

  def test_default_user_agent_pattern
    config = TurboDesktop::Configuration.new
    assert_equal(/Turbo Desktop/, config.user_agent_pattern)
  end

  def test_default_path_configuration_has_settings
    config = TurboDesktop::Configuration.new
    assert_equal false, config.path_configuration[:settings][:screenshots_enabled]
  end

  def test_default_path_configuration_has_rules
    config = TurboDesktop::Configuration.new
    rules = config.path_configuration[:rules]
    assert_equal 1, rules.length
    assert_equal [ "/" ], rules.first[:patterns]
    assert_equal "default", rules.first[:properties][:presentation]
  end

  # --- Setters ---

  def test_user_agent_pattern_is_configurable
    config = TurboDesktop::Configuration.new
    config.user_agent_pattern = /CustomApp/
    assert_equal(/CustomApp/, config.user_agent_pattern)
  end

  def test_path_configuration_is_configurable
    config = TurboDesktop::Configuration.new
    custom = {
      settings: { screenshots_enabled: true },
      rules: [
        { patterns: [ "/new" ], properties: { presentation: "modal" } }
      ]
    }
    config.path_configuration = custom
    assert_equal custom, config.path_configuration
  end

  # --- path_configuration_json ---

  def test_path_configuration_json_returns_valid_json
    config = TurboDesktop::Configuration.new
    json = config.path_configuration_json
    parsed = JSON.parse(json)
    assert_kind_of Hash, parsed
    assert parsed.key?("settings")
    assert parsed.key?("rules")
  end

  def test_path_configuration_json_reflects_custom_config
    config = TurboDesktop::Configuration.new
    config.path_configuration = {
      settings: { screenshots_enabled: true },
      rules: [
        { patterns: [ "/modal" ], properties: { presentation: "modal" } },
        { patterns: [ "/" ], properties: { presentation: "default" } }
      ]
    }
    parsed = JSON.parse(config.path_configuration_json)
    assert_equal 2, parsed["rules"].length
    assert_equal "modal", parsed["rules"].first["properties"]["presentation"]
  end

  # --- TurboDesktop.configure DSL ---

  def test_configure_block_sets_user_agent_pattern
    TurboDesktop.configure do |config|
      config.user_agent_pattern = /MyApp/
    end
    assert_equal(/MyApp/, TurboDesktop.configuration.user_agent_pattern)
  end

  def test_configure_block_sets_path_configuration
    custom_rules = {
      settings: {},
      rules: [ { patterns: [ "/admin" ], properties: { presentation: "native" } } ]
    }
    TurboDesktop.configure do |config|
      config.path_configuration = custom_rules
    end
    assert_equal custom_rules, TurboDesktop.configuration.path_configuration
  end

  def test_configuration_returns_same_instance
    config1 = TurboDesktop.configuration
    config2 = TurboDesktop.configuration
    assert_same config1, config2
  end

  def test_reset_configuration_creates_new_instance
    old_config = TurboDesktop.configuration
    TurboDesktop.reset_configuration!
    new_config = TurboDesktop.configuration
    refute_same old_config, new_config
  end

  def test_reset_configuration_restores_defaults
    TurboDesktop.configure do |config|
      config.user_agent_pattern = /CustomPattern/
    end
    TurboDesktop.reset_configuration!
    assert_equal(/Turbo Desktop/, TurboDesktop.configuration.user_agent_pattern)
  end

  def test_inspector_disabled_by_default
    config = TurboDesktop::Configuration.new
    refute config.inspector_enabled
  end

  def test_inspector_can_be_enabled
    config = TurboDesktop::Configuration.new
    config.inspector_enabled = true
    assert config.inspector_enabled
  end
end
