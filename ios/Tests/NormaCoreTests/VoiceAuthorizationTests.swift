import XCTest
@testable import NormaCore

final class VoiceAuthorizationTests: XCTestCase {
    /// 缓存语音授权只属于签发电脑和固定服务，到期后必须重新经配对领取。
    /// 数据构造：iat=1000，exp=1000+24×60×60=87400；签名用70字节占位，仅测本地缓存解析。
    /// 执行过程：读取合法缓存→换电脑→到期→错服务→损坏token；真实验签由Rust→Node的TLS e2e覆盖。
    /// 预期结果：有效授权可离线使用；跨电脑、过期、跨服务和损坏记录不应进入请求头。
    func testCachedVoiceAccessChecksBindingAndExpiry() throws {
        let machine="mac-"+String(repeating:"a",count:64)
        func token(audience:String = "https://soulvoiceai.com/norma/voice") throws -> String {
            let payload=try JSONSerialization.data(withJSONObject:["v":1,"iss":machine,"sub":"phone-a",
                "aud":audience,"iat":1000,"exp":87400])
            return "nva1."+payload.map {String(format:"%02x",$0)}.joined()+"."+String(repeating:"ab",count:70)
        }
        let valid=try token()
        let grant=try XCTUnwrap(VoiceAuthorization.cached(valid,machineID:machine,now:1000),"合法缓存应可使用")
        XCTAssertEqual(grant.deviceID,"phone-a","保留原签发设备")
        XCTAssertEqual(grant.expiresAt,87400,"缓存不得延长签名有效期")
        XCTAssertNotNil(VoiceAuthorization.cached(try token(audience:"https://norma-voice.vercel.app"),now:1000),"迁移期间旧Mac的有效签名仍可使用")
        XCTAssertNil(VoiceAuthorization.cached(valid,machineID:"mac-"+String(repeating:"b",count:64),now:1000),"不能把A电脑签发的授权转给B")
        XCTAssertNil(VoiceAuthorization.cached(valid,now:87400),"到期瞬间不能继续发送旧授权")
        XCTAssertNil(VoiceAuthorization.cached(try token(audience:"https://other.example"),now:1000),"其他服务的授权不能发送给默认网关")
        XCTAssertNil(VoiceAuthorization.cached("shared-secret",now:1000),"共享口令不能被当作自动设备授权")
    }

    /// 服务迁移只替换已知旧默认值，保留用户自定义部署。
    /// 数据构造：未配置、旧Norma/Companion默认值和一个自定义子路径；无时间或预算参数。
    /// 执行过程：逐一计算启动地址；生产常量必须是新服务器路径。
    /// 预期结果：默认与旧版本自动免配置迁移，自定义地址不被覆盖。
    func testGatewayMigrationPreservesCustomDeployments() {
        XCTAssertEqual(VoiceAuthorization.serviceOrigin,"https://soulvoiceai.com/norma/voice","新安装默认应连接自有服务器")
        XCTAssertEqual(VoiceAuthorization.initialGateway(nil),"https://soulvoiceai.com/norma/voice","未配置时自动使用新服务")
        XCTAssertEqual(VoiceAuthorization.initialGateway("https://norma-voice.vercel.app"),"https://soulvoiceai.com/norma/voice","旧Norma默认应迁移")
        XCTAssertEqual(VoiceAuthorization.initialGateway("https://milltina-voice.vercel.app"),"https://soulvoiceai.com/norma/voice","早期Companion默认应迁移")
        XCTAssertEqual(VoiceAuthorization.initialGateway("https://custom.example/voice"),"https://custom.example/voice","不得覆盖用户自定义服务")
        XCTAssertNotEqual(VoiceAuthorization.initialGateway(nil),"https://norma-voice.vercel.app","新安装不能再依赖Vercel入口")
    }
}
