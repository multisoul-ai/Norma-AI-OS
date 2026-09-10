import Foundation

/// Metadata only. Credentials live exclusively in CredentialVault.
public actor MachineStore {
    private let url: URL
    public init(url: URL) { self.url = url }

    public func load() throws -> [PairedMachine] {
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        // At most 128 saved Macs × four endpoints and display metadata, bounded to 256 KiB.
        let data = try Data(contentsOf: url)
        guard data.count <= 256 * 1024 else { throw MobileError.invalidResponse }
        let machines = try JSONDecoder().decode([PairedMachine].self, from: data)
        guard machines.count <= 128, Set(machines.map(\.id)).count == machines.count else {
            throw MobileError.invalidResponse
        }
        try machines.forEach { try $0.validate() }
        return machines
    }
    public func save(_ machine: PairedMachine) throws {
        try machine.validate()
        var machines = try load()
        if let index = machines.firstIndex(where: { $0.id == machine.id }) { machines[index] = machine }
        else { machines.append(machine) }
        guard machines.count <= 128 else { throw MobileError.invalidResponse }
        try write(machines)
    }
    public func remove(id: String) throws { try write(load().filter { $0.id != id }) }
    private func write(_ machines: [PairedMachine]) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(machines).write(to: url, options: .atomic)
    }
}
