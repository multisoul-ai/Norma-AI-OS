#!/usr/bin/env python3
"""发布可用性检查与 App 下载器必须采用相同的 HTTPS 重定向边界。"""
import importlib.util
from pathlib import Path
import unittest
import urllib.request

SPEC = importlib.util.spec_from_file_location("verification", Path(__file__).with_name("verify-release-media.py"))
verification = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verification)


class RedirectTests(unittest.TestCase):
    # 场景：HTTPS 原资源重定向到 HTTP，再跳回 HTTPS。
    # 构造第一跳 request；即使最终地址安全，第一跳也必须立即拒绝。
    def test_intermediate_http_redirect_is_rejected(self):
        request = urllib.request.Request("https://example.com/start")
        with self.assertRaises(ValueError, msg="发布 gate 不能放行 App 拒绝的中间 HTTP 跳转"):
            verification.TrustedRedirects().redirect_request(request, None, 302,
                "Found", {}, "http://example.com/middle")

    # 场景：正常 GitHub HTTPS CDN 重定向。
    # 预期仍允许 HTTPS 原文件跳转，避免收紧规则导致合法下载失败。
    def test_https_redirect_remains_supported(self):
        request = urllib.request.Request("https://example.com/start")
        redirect = verification.TrustedRedirects().redirect_request(request, None, 302,
            "Found", {}, "https://cdn.example.com/original.mp4")
        self.assertEqual(redirect.full_url, "https://cdn.example.com/original.mp4",
            "安全 CDN 跳转必须保留")

    # 场景：重复相同 HTTPS URL 的链，不能只按 distinct URL 数量限制。
    # App 允许前 10 跳，previous.len()=10 时拒绝第 11 跳；gate 采用相同请求计数。
    def test_every_redirect_hop_counts_towards_the_limit(self):
        request = urllib.request.Request("https://example.com/start")
        handler = verification.TrustedRedirects()
        for _ in range(10):
            request = handler.redirect_request(request, None, 302,
                "Found", {}, "https://example.com/repeated")
        with self.assertRaises(ValueError, msg="第 11 跳必须与 App 一致被拒绝"):
            handler.redirect_request(request, None, 302,
                "Found", {}, "https://example.com/repeated")


if __name__ == "__main__":
    unittest.main()
