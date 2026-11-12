#!/usr/bin/env ruby
# Require immutable remote actions in workflows and repository-local actions.
require 'yaml'
require 'date'
require 'open3'

paths = ARGV
if paths.empty?
  tracked, status = Open3.capture2('git', 'ls-files', '-z')
  abort 'Cannot list tracked files' unless status.success?
  paths = tracked.split("\0").select do |path|
    path.match?(%r{\A\.github/workflows/[^/]+\.ya?ml\z}) ||
      path.match?(%r{(?:\A|/)action\.ya?ml\z})
  end
end

def check(node, path, errors, visited)
  return unless node.is_a?(Hash) || node.is_a?(Array)
  return if visited[node.object_id]

  visited[node.object_id] = true
  if node.is_a?(Hash)
    if node.key?('uses')
      ref = node['uses']
      pinned = ref.is_a?(String) && (
        ref.start_with?('./') ||
        ref.match?(%r{\A[\w.-]+/[\w.-]+(?:/[^\s@]+)?@[0-9a-fA-F]{40}\z}) ||
        ref.match?(%r{\Adocker://[^\s@]+@sha256:[0-9a-fA-F]{64}\z})
      )
      errors << "#{path}: #{ref.inspect} must use a full commit SHA (or Docker SHA-256 digest)" unless pinned
    end
    node.each_value { |value| check(value, path, errors, visited) }
  else
    node.each { |value| check(value, path, errors, visited) }
  end
end

errors = []
paths.each do |path|
  begin
    data = YAML.safe_load(File.read(path), permitted_classes: [Date, Time], aliases: true)
    # SLSA verifies the reusable workflow identity through its release tag.
    # https://github.com/slsa-framework/slsa-github-generator/tree/v2.1.0#referencing-slsa-builders-and-generators
    if path == '.github/workflows/esp-desktop-release.yml'
      provenance = data.dig('jobs', 'provenance')
      if provenance.is_a?(Hash) && provenance['uses'] == 'slsa-framework/slsa-github-generator/.github/workflows/generator_generic_slsa3.yml@v2.1.0'
        warn "#{path}: allowing the documented SLSA v2.1.0 tag exception"
        data['jobs']['provenance'] = provenance.reject { |key, _| key == 'uses' }
      end
    end
    check(data, path, errors, {})
  rescue Psych::Exception => error
    errors << "#{path}: #{error.message}"
  end
end
abort errors.join("\n") unless errors.empty?
puts "Action references satisfy the pin policy (#{paths.length} files checked)."
