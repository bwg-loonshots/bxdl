# 제품 설정과 NIGO native 설정의 결합 preflight

- 작성: 2026-09-18
- 제품 기반: BXDL `cb97b2713719f1d5948016c27f5f96dfb77b959d` 이후의 결합 검사
- 공급 기준: [clean 개발 후보 수신 기록](../contracts/nigo/development-clean-2026-09-18/README.md), NIGO PR #119 문서 checkout `49d1cefcfb200f8eb04b6f9961c889da21f81018`; JAR source `303e163a9b3f293fa39e42d02b8daa1843973c14`, dirty=false
- 계약 수준: PROPOSED/development/officialRelease=false. 이 문서는 BXDL 소비 방식이며 NIGO 원장·canonical 규칙·실행 권한을 변경하지 않는다.

## 1. 해결할 사용자 흐름

기존 setup은 제품 설정을 저장하고 install은 새 폴더에 검증한 파일을 설치한다. 독립 engine preflight는 운영자가 별도로 준비한 NIGO native 설정만 검사한다. 이 상태에서는 잘못된 다른 노드의 native 설정을 검사하고 setup 설정도 검사했다고 오해할 수 있다.

이번에는 저장한 제품 설정과 native 설정의 대응 값이 같다는 근거를 엔진 cold 결과와 함께 반환한다. 사용자는 setup export → 필요 시 독립 install → 결합 preflight 순으로 진행한다. setup 질문에 package 설치나 native 생성을 붙이지 않고, 기존 제품 schema와 초안 format도 변경하지 않는다.

범위는 읽기·metadata·입력 일치 확인과 명시 cold JVM 호출이다. 자동 native 렌더, PKI 생성, 설치 등록·instance journal, init/resume-init/run, launchd, 상태·정상 종료·G1-M 인수는 별도 후속이다. 기존 BX-020/023/030과 R1~R3의 연결을 보강하며 전체 작업이나 gate를 DONE으로 올리는 근거가 아니다.

## 2. CLI와 내부 진입점

```text
bxdl preflight --config <instance.json> [--json]

bxdl preflight --config <instance.json> --engine-config <node.json>
    --jar <jar> --java <absolute-java> --lock <trusted-engine.lock.json>
    --allow-development [--timeout-seconds <1..120>] [--json]
```

엔진 옵션이 없으면 기존 metadata-only 결과 구조와 exit 의미를 유지한다. 엔진 옵션은 `engine-config/jar/java/lock/allow-development`가 전부 있어야 한다. 일부만 지정하거나 timeout만 추가하는 조합은 인자 오류다. timeout 기본은 30초이며 engine-info와 cold preflight 각각에 적용한다. Java는 절대경로를 요구한다.

내부 진입점은 다음과 같다. CLI는 옵션 조합과 개발 후보 실행 opt-in을 검사하고, 엔진 모듈은 release/READY를 주장하지 않는다.

```rust
pub fn preflight_product(
    options: &engine::Options,
    instance: &Path,
    native_config: &Path,
) -> Result<engine::ProductReport>;
```

`ProductReport`의 직렬화 필드는 `outcome`, `product`, `configurationBinding`, 선택 `engine`이다. product에는 기존 config::Report, engine에는 기존 engine::Report를 사용한다. 입력 문서나 secret 경로를 보고서에 추가하지 않는다.

## 3. 14개 제품 입력의 정확한 대응

아래 표에서 `Q`는 native node의 flat key 접두사 `nigo.protocol.consensus.qbft.node.`다. native root는 `chainFile/dataDirectory/backend/node` 네 필드이며 제품 JSON과 다른 형식이다.

| setup 입력 | 제품 필드 | native 대응 |
| --- | --- | --- |
| 1. 인스턴스 이름 | instanceId | 없음. 제품 보고서 식별자로만 보존 |
| 2. 공개 node ID | nodeId | node[Q + node-id] |
| 3. data 위치 | storage.dataDirectory | root dataDirectory |
| 4. HTTP IP | http.address | node[server.address] |
| 5. HTTP port | http.port | node[server.port] |
| 6. P2P IP | p2p.address | node[Q + listen-host] |
| 7. P2P port | p2p.port | node[Q + listen-port] |
| 8. 공개 chain 자료 | chainDescription | root chainFile |
| 9. validator keystore | secrets.validatorKeystore | node[Q + keystore-path] |
| 10. validator password file | secrets.validatorPasswordFile | node[Q + keystore-password-file] |
| 11. TLS keystore | secrets.tlsKeyStore | node[Q + mtls-key-store-path] |
| 12. TLS key password file | secrets.tlsKeyPasswordFile | node[Q + mtls-key-store-password-file] |
| 13. TLS truststore | secrets.tlsTrustStore | node[Q + mtls-trust-store-path] |
| 14. TLS trust password file | secrets.tlsTrustPasswordFile | node[Q + mtls-trust-store-password-file] |

추가로 제품의 고정 `role=validator`는 native `node[Q + role]=VALIDATOR`, `storage.backend=rocksdb`는 root backend와 일치해야 한다. native `node[Q + transport-security-scheme]=MTLS`, 참조한 공개 chain의 `nigo.protocol.consensus.protocol=QBFT`를 명시 요구한다. INSTANT smoke나 OBSERVER를 제품 validator의 검사로 받아들이지 않는다.

이 대응 값은 **명시 필수**이며 누락을 엔진 default로 채우지 않는다. NIGO가 일반 native 입력에서 HTTP·listen·role 기본값을 제공하는 것과 BXDL 결합 정책은 별개다. QBFT/VALIDATOR/MTLS의 표기는 위 대문자 exact value를 요구한다.

node ID는 `0x` + 64 hex이며 본문의 대소문자를 정규화해 비교한다. 제품의 일반 식별자나 placeholder를 canonical ID로 간주하지 않는다. 주소는 IP 값으로 비교하며 DNS 조회를 하지 않는다. 포트는 1~65535의 JSON 정수 또는 숫자 문자열로 비교하고 부호·소수·bool을 거부한다.

제품 참조는 instance.json의 위치, native 참조는 node.json의 위치에서 각각 절대화한다. 서로 다른 상대경로라도 같은 허용 위치를 가리킬 수 있다. Mac 경로 별칭 검사와 symlink 거부를 유지하며 product 설정이 native data 아래에 위치하는 것도 거부한다. 같은 내용의 파일이라는 이유로 별도 경로를 임의 대체하지 않는다.

## 4. 제품 v1에 없는 입력

native의 다음 자료는 이번 일치 비교에 대응하는 제품 필드가 없다. 운영자가 명시 native 파일로 준비하고 NIGO가 계약대로 검증한다. 이를 제품 설정에서 생성하거나 모든 node-local 정책이 비교됐다고 주장하지 않는다.

- validator ID: `0x` + 40 hex. node ID와 다르며 canonical chain membership과 signer key의 일치는 NIGO가 검증한다.
- peers: node ID·host/port·role·validator ID, transaction permission(NONE/RELAY/INGRESS), active/revoked credential fingerprints.
- TLS store type: PKCS12 또는 JKS. NIGO의 type 기본값은 PKCS12이며 제품 v1에 별도 type가 없으므로 그대로 엔진에 위임한다.
- sync enabled/trust mode와 선택 trusted checkpoint, archive serving, local ingress permission, console와 consensus auto-start 등 node-local 정책.

peer 수나 sync 활성화를 cold 성공과 같은 뜻으로 보지 않는다. peer 연결·quorum·catch-up은 런타임 확인 항목이다. 공개 chain의 validator set·genesis·fee profile과 node의 peer/secret/sync/ingress 설정 책임을 섞지 않는다.

Validator keystore는 NIGO `EncryptedValidatorKeystore`의 NGVK revision-1 PBKDF2/AES-GCM binary다. TLS PKCS12/JKS와 호환되는 일반 keystore가 아니다. 기존 제품 예제의 `.p12` validator 파일명은 경로 placeholder이며 이번 문서 작업은 예제 JSON bytes를 바꾸지 않는다.

mTLS pin은 `TransportCredentialFingerprint`가 만드는 도메인 분리 값이다. NIGO source 기준 식은 `SHA256(ASCII("NIGO_TRANSPORT_CREDENTIAL/v1") || 0x00 || ASCII("MTLS") || 0x00 || DER(leaf certificate))`이며 결과는 32-byte hash다. 일반 X.509 fingerprint 또는 SPKI hash를 대체 입력으로 사용하지 않는다. BXDL이 임의 pin을 계산해 승인하거나 키를 발급하는 기능은 이번 범위에 없다.

## 5. 실행·입력 결속

1. 제품 파일을 크기 제한·경로/파일 identity 검사 아래 읽고 기존 제품 schema로 정규화한다. local report의 configSha256과 이 입력 bytes를 대조한다.
2. 로컬 metadata가 FAIL이면 product report를 보존해 반환한다. native/engine 검사를 실행하지 않는다.
3. 신뢰 lock과 native config/공개 chain을 읽는다. 같은 NativeInput의 bytes·해석된 경로로 제품 대응 값을 검사한 뒤 snapshot을 만든다. 별도 비교용 파일과 실행용 재읽기가 분리되지 않게 한다.
4. private workspace에 JAR를 size/hash 검증하며 복사하고 Java pin을 확인한다. env_clear·bounded stdout/stderr·timeout/kill/reap을 유지한다. 실제 engine-info 전체 identity가 lock과 일치해야 cold로 진행한다.
5. native config·공개 chain snapshot으로 cold를 호출한다. 원본 recheck와 제품 파일/hash·metadata 확인을 유지한다. 응답의 node identity가 제품 ID와 같은 validator이고 backend가 같은지도 대조한다. 입력 변경·응답 불일치를 정상 결과로 승격하지 않는다.

raw config SHA와 NIGO의 canonical chainFingerprint는 다른 값이다. engine report의 configSha256/chainFileSha256은 원본 입력 bytes를 가리키고, chainFingerprint는 엔진이 산출한 canonical 의미다. 서로 대체하지 않는다.

secret 내용은 BXDL 결과·로그에 넣지 않는다. metadata-only 경로는 key content를 읽지 않지만 명시 cold JVM은 signer password와 TLS material을 읽는다. KEY_MATERIAL PASS는 서명·handshake·expiry/revocation·네트워크 인수가 아니다. DB/WAL도 열지 않는다. 입력과 process 출력 처리는 동일 UID 공격자에 대한 강한 sandbox를 제공하지 않는다.

## 6. 결과와 실패 의미

| 경로/조건 | envelope outcome | exit | data 의미 |
| --- | --- | --- | --- |
| 로컬 전용, metadata 정상 | INCOMPLETE | 5 | 기존 config report 자체, 엔진 미실행 |
| 로컬 전용, read/schema 오류 | FAILED | 2 | 기존 오류 처리 |
| 결합 옵션 일부 누락·timeout 범위 오류 | FAILED | 2 | 인자 오류, 실행 안 함 |
| 결합, local metadata FAIL | FAILED | 4 | ProductReport.outcome=FAIL, product 실패 보존, binding NOT_CHECKED, engine 없음 |
| 제품/native 대응 불일치 | FAILED | 3 | ENGINE_PRODUCT_MISMATCH, 성공 보고서 없음 |
| 결합 input/schema·lock/pin·response 오류 | FAILED | 3 | 정제된 reasonCode, 성공 보고서 없음 |
| 결합 일치 + 정상 cold | INCOMPLETE | 5 | ProductReport.outcome=INCOMPLETE, binding MATCHED, engine 포함 |
| JVM timeout | UNKNOWN | 6 | ENGINE_TIMEOUT, 성공 보고서 없음 |

binding 실패는 JVM 실행 전 확인한다. 단, 실제 엔진 응답 identity가 제품과 다르면 이미 수행한 cold의 결과를 거부한다. 두 경우 모두 `ENGINE_PRODUCT_MISMATCH`를 사용할 수 있으므로 reasonCode 하나만으로 JVM 미실행 여부를 추론하지 않는다.

결합 report의 product에는 기존 로컬 관측을 그대로 남기며 engine이 통과한 항목으로 수정하지 않는다. engine 결과의 NOT_CHECKED도 유지한다. config·키 경로·child stderr를 오류에 되돌려 쓰지 않는다. JSON stdout은 envelope 하나이며 비대화형 실행 stderr는 비어 있다. `MATCHED`나 exit 5를 READY/정상 종료/초기화 허가로 해석하지 않는다.

## 7. clean 후보와 후속 책임

새 공급 JAR는 `dd5a366ee990d58ff4fa7da89812f66ec9025443229027bffe1b648cab6d6455`다. 새 source/dirty/size/계약 fingerprint는 [clean snapshot](../contracts/nigo/development-clean-2026-09-18/README.md)과 새 외부 lock에 결속한다. [이전 dirty 후보](../contracts/nigo/development-2026-09-17/README.md)의 원문·검증 기록은 이력으로 유지한다. 문서 HEAD, 공급 source, 실제 JAR hash와 BXDL product revision을 구분한다.

공급자는 새 후보의 종료 drain·follower source 전환·동일 데이터 재시작을 검증했다. 그 결과는 선정 BXDL JRE/package·launchd·오프라인·4-validator G1-M 시험을 대신하지 않는다. 정식 JRE 공급·LICENSE/NOTICE/SBOM, OS 최소 버전과 전체 운영 인수도 남아 있다.

BXDL 제품 후속 작업은 source-free QBFT provisioning과 거래·동일 finalized 높이 비교 fixture의 입력 준비를 맡는다. 공개 chain/validator membership·node 설정·signer/password·mTLS trust/pin의 발급·배치 입력을 갖추되 NIGO canonical/키·합의 검증을 복제하거나 NIGO 원장 상태를 변경하지 않는다. 현재 CLI에는 generator가 없다.

자동 mapping을 도입할 때는 validator ID·peer/pin·TLS type·sync 정책을 표현하는 versioned product schema와 명시 migration을 별도로 설계한다. 기존 v1이나 불완전 draft를 자동으로 실행 가능한 QBFT 설정으로 승격하지 않는다. 이번에는 v1/초안 migration이 없다.

## 8. 검증 범위

이번 연결의 검증은 일치하는 QBFT fixture의 exit 5, 각 대응값 불일치·누락·INSTANT/observer/plaintext 거부, local FAIL의 JVM 미실행, 원본 상대경로 보존·변경 탐지, timeout/오류 envelope와 secret canary 비노출을 포함한다. schema/옵션 혼합·숫자 타입·node ID 형식과 JSON 단일 출력도 확인한다.

이는 검증 요구 범위이며 실행 결과 숫자를 뜻하지 않는다. 실제 명령·환경·제품 revision·candidate/runtime hash 및 미실행 항목은 [구현 상태](../docs/implementation-status.md)와 연결된 결과 기록을 따른다. source-free QBFT input 준비가 없으면 INSTANT cold 성공을 결합 QBFT 인수로 대신 기록하지 않는다.

## 9. 코드·계약 근거

- BXDL: `src/config/mod.rs`, `src/config/preflight.rs`, `src/setup/mod.rs`, `src/engine/{product,mod,files}.rs`, `src/cli.rs`.
- [NIGO 명령 계약](../contracts/nigo/development-clean-2026-09-18/ENGINE_CONTRACT.md), [runtime 계약](../contracts/nigo/development-clean-2026-09-18/ENGINE_RUNTIME_CONTRACT.md).
- NIGO source `303e163a…`: `nigo-java/nigo-node/src/main/java/org/nigo/node/engine/{EngineConfiguration,EnginePreflight}.java`, `config/{QbftNodeProperties,QbftSyncProperties,QbftSyncArchiveProperties,TransactionIngressProperties}.java`, `consensus/signer/EncryptedValidatorKeystore.java`.
- NIGO pin 형식: `nigo-java/nigo-network-p2p/src/main/java/org/nigo/protocol/network/{TransportCredentialFingerprint,MtlsSecurePeerTransport}.java`.
