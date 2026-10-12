# Read installed formula recipes through Homebrew's own platform-aware DSL.
# No fetch, patch, install or post-install methods are invoked.
require "formulary"
require "json"

def resource_record(resource)
  { name: resource.name, url: resource.url, checksum: resource.checksum.to_s,
    specs: resource.specs }
end

records = ARGV.map do |path|
  formula = Formulary.factory(Pathname.new(path))
  patches = formula.stable.patches.map do |patch|
    record = { type: patch.class.name, strip: patch.strip.to_s }
    if patch.is_a?(ExternalPatch)
      record.merge!(resource_record(patch.resource))
    elsif defined?(LocalPatch) && patch.is_a?(LocalPatch)
      record[:file] = patch.file.to_s
    elsif patch.is_a?(EmbeddedPatch)
      patch.path = Pathname.new(path) if patch.is_a?(DATAPatch)
      record[:contents] = patch.contents
    else
      raise "Unreviewed formula patch type: #{patch.class.name}"
    end
    record[:directory] = patch.directory.to_s if patch.respond_to?(:directory)
    record
  end
  { name: formula.name, version: formula.pkg_version.to_s, stable_version: formula.stable.version.to_s,
    license: formula.license,
    stable: resource_record(formula.stable), resources: formula.resources.map { |r| resource_record(r) },
    patches: patches }
end
puts JSON.generate(records)
