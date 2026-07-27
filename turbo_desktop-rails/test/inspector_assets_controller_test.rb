require "test_helper"

# Uses the shared app + mounted engine from test_helper.rb.
class InspectorAssetsControllerTest < ActionDispatch::IntegrationTest
  def test_serves_inspector_entry_module
    get "/turbo-desktop/inspector.js"
    assert_response :success
    assert_equal "text/javascript", response.media_type
    assert_includes response.body, "startInspector"
  end

  def test_serves_every_whitelisted_module
    %w[inspector.js inspector/state.js inspector/panel.js inspector/bridge-tap.js inspector/catalog.js].each do |asset|
      get "/turbo-desktop/#{asset}"
      assert_response :success, "expected #{asset} to be served"
      assert_equal "text/javascript", response.media_type
    end
  end

  def test_submodule_body_is_the_real_module
    get "/turbo-desktop/inspector/state.js"
    assert_includes response.body, "class InspectorState"
  end

  def test_unknown_module_is_not_found
    get "/turbo-desktop/inspector/nope.js"
    assert_response :not_found
  end

  def test_non_whitelisted_ruby_file_is_not_found
    get "/turbo-desktop/inspector/engine.rb"
    assert_response :not_found
  end
end

# Guards against the vendored copy drifting from the canonical shell source.
# In the monorepo ../../src is present; in a packaged gem it is not (test skips).
class InspectorAssetDriftTest < Minitest::Test
  SHELL_SRC  = File.expand_path("../../src", __dir__)
  GEM_ASSETS = File.expand_path("../lib/turbo_desktop/inspector_assets", __dir__)
  FILES = %w[inspector.js inspector/state.js inspector/panel.js inspector/bridge-tap.js inspector/catalog.js].freeze

  def test_vendored_assets_match_shell_source
    FILES.each do |file|
      shell = File.join(SHELL_SRC, file)
      next unless File.exist?(shell) # packaged gem: no shell src to compare against

      assert_equal File.read(shell), File.read(File.join(GEM_ASSETS, file)),
                   "#{file} drifted from shell src/#{file} — run `rake turbo_desktop:sync_inspector`"
    end
  end
end
