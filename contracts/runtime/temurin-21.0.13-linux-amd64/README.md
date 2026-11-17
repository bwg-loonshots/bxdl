# Temurin 21.0.13+9 Linux x64 full JRE 공급 기록

2026-11-16 공식 [고정 릴리즈](https://github.com/adoptium/temurin21-binaries/releases/tag/jdk-21.0.13%2B9)의 Linux x64/glibc full JRE를 수신했다. Linux를 새로 인수하므로 이날 확인한 Java21 LTS 패치 21.0.13+9를 선정했다. Mac의 21.0.12.1+1 고정 조합은 변경하지 않는다. 버전 URL과 byte identity를 고정하며 자동 latest 선택은 없다.

[Linux runtime lock](../../../packaging/runtime-lock.temurin-21-linux-amd64.json), [RC.4 Linux engine lock](../../../packaging/engine-lock.rc4-temurin-linux-development.json), [조합 identity](../../../packaging/linux-rc4-candidate.json)를 함께 사용한다. 이것은 공급물 선정이며 운영 runtime archive 또는 허용된 executor profile이 아니다.

| 고정 입력 | SHA-256 / 크기 |
| --- | --- |
| JRE archive | `a535a1df9c6284d9618872984e026ae0352d1f7e9e0605425c0e6114f47351dd` / 52,181,297 bytes |
| 대응 OpenJDK source | `b5232c9980d8a0f0459676c084c41c6e2c771ed9e785890b03c6a438406ad1ca` / 115,231,532 bytes |
| Linux bin/java | `7afc28801f744050aeba646b20e7314265279f76f9d0273b21a2b66411e1bf6d` / 13,024 bytes |
| 전체 runtime lock | `23eec871655dea8af8f79cb69f21c4add762226a9314df003edce1233e898f47` |
| 고정 RC.4 JAR | `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` / 164,133,636 bytes |

320개 tar entry는 112개 정규파일·63개 디렉터리·145개 내부 legal 파일 symlink다. 링크를 정규파일로 펼친 전체 inventory는 **257파일·166,678,666 bytes**다. 내용은 유지하고 mode는 실행 bit에 따라0644/0755로 정규화한다.0444 원본 고지 파일도0644로 준비한다. Home root는 `jdk-21.0.13+9-jre`이며 Mac의 `Contents/Home` 구조와 다르다.

| 기록 | 범위 |
| --- | --- |
| [received-supply.json](./received-supply.json) | 공식 GitHub asset11개 size/digest·별도 checksum 대조, source/build자료 수신과 고지9파일 실제 읽기 |
| [supplier-metadata.json](./supplier-metadata.json), [supplier-sbom.json](./supplier-sbom.json) | 공급자 원본 보존 |
| [observed-supply.json](./observed-supply.json) | 오프라인 tar·materialized inventory·링크 관계. `release MODULES`는 build JDK 선언이며 실제 JRE 실행 모듈 목록이 아님 |
| [source-supply.json](./source-supply.json) | 대응 source/build commit·archive와 로컬 보존/고객 제공 범위 구분 |

이 Linux SBOM의 JRE component archive hash는 실제 공급물과 일치한다. Mac의 별도 SBOM 불일치를 해결했다는 뜻이 아니며 전체 제품 SBOM 인수도 아니다. 공급 release는 immutable=false이므로 같은 URL의 다른 bytes는 고정 hash에서 거절한다. HTTPS metadata/checksum 일치는 detached signature 검증과 다르며 서명 검증은 미수행이다.

공급 고지 원문과 대응 source/build 자료를 보존했다. 제품 전체 NOTICE/SBOM·고객 source 전달·정식 출시 승인은 별도다. Linux Java/engine-info/native/RocksDB·서비스 실행은 **NOT_RUN**이다. [LNX-02 결과](../../../results/2026-11-16-linux-rc4-supply.md)와 [테스터 실행 안내](../../../docs/linux-validation.md)를 따른다. SSH 정보 공유 없이 테스터가 실행하고 정제 결과 JSON을 전달할 수 있다.
