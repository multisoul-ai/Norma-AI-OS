import XCTest
@testable import NormaCore

final class VoiceAuthorizationTests: XCTestCase {
    /// 缓存语音授权只属于签发电脑和固定服务，到期后必须重新经配对领取。
    /// 数据构造：iat=1000，exp=1000+24×60×60=87400；签名用70字节占位，仅测本地缓存解析。
    /// 执行过程：读取合法缓存→换电脑→到期→错服务→损坏token；真实验签由Rust→Node的TLS e2e覆盖。
    /// 预期结果：有效授权可离线使用；跨电脑、过期、跨服务和损坏记录不应进入请求头。
    func testCachedVoiceAccessChecksBindingAndExpiry() throws {
        let machine="mac-"+String(repeating:"a",count:64)
        func token(audience:String = "https://norma-voice.vercel.app") throws -> String {
            let payload=try JSONSerialization.data(withJSONObject:["v":1,"iss":machine,"sub":"phone-a",
                "aud":audience,"iat":1000,"exp":87400])
            return "nva1."+payload.map {String(format:"%02x",$0)}.joined()+"."+String(repeating:"ab",count:70)
        }
        let valid=try token()
        let grant=try XCTUnwrap(VoiceAuthorization.cached(valid,machineID:machine,now:1000),"合法缓存应可使用")
        XCTAssertEqual(grant.deviceID,"phone-a","保留原签发设备")
        XCTAssertEqual(grant.expiresAt,87400,"缓存不得延长签名有效期")
        XCTAssertNil(VoiceAuthorization.cached(valid,machineID:"mac-"+String(repeating:"b",count:64),now:1000),"不能把A电脑签发的授权转给B")
        XCTAssertNil(VoiceAuthorization.cached(valid,now:87400),"到期瞬间不能继续发送旧授权")
        XCTAssertNil(VoiceAuthorization.cached(try token(audience:"https://other.example"),now:1000),"其他服务的授权不能发送给默认网关")
        XCTAssertNil(VoiceAuthorization.cached("shared-secret",now:1000),"共享口令不能被当作自动设备授权")
    }
}
