// Specs import .js, matching the compiled modules. This loader reads the .ts source.
export async function resolve(specifier, context, nextResolve) {
	if ((specifier.startsWith('./') || specifier.startsWith('../')) && specifier.endsWith('.js')) {
		try {
			return await nextResolve(`${specifier.slice(0, -3)}.ts`, context)
		} catch {
			return nextResolve(specifier, context)
		}
	}
	return nextResolve(specifier, context)
}
