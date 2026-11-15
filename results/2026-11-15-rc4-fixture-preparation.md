# RC4-02 계약 영향·새 시험 자료 준비 — 2026-11-15

**RC4-02 구현·회귀·실물 준비 PASS. RC.4 제품 선택·runtime 조립·노드/DB/서비스/네트워크 인수는 미실행이다.**
[영향 설계](../design/2026-11-15-rc4-contract-fixture-impact.md), [구조화 결과](./2026-11-15-rc4-fixture-preparation.json), [다음 단계 계획](../design/2026-11-15-rc4-adoption-plan.md)을 따른다.

## 구현과 기준

- 기준 BXDL commit은 `66c928936b54acd74bf3bab9acb033b44e0e7729`이며 RC4-01 [PR #22](https://github.com/bwg-loonshots/bxdl/pull/22) 머지 뒤의 미커밋 RC4-02 변경으로 시험했다. 실제 harness 파일 hash·실행 시 tracked diff hash를 JSON에 기록했고 clean commit 결과로 표현하지 않는다.
- NIGO source `037c98c315d5322fa7da266ca0254d61f3863622`, 수신 JAR `56887aa12f2386f7211b11d56506aa42ab392c09f46c40f1431893ce77f6fa0a` / 164,133,636bytes를 그대로 사용했다. 엔진을 재빌드하지 않았다. Temurin21.0.12.1+1-LTS full JRE259파일도 기존 lock 그대로다.
- 실행 후보 `CANDIDATES`는 RC1~3를 유지하고 준비 후보 `PREPARATION_CANDIDATES`에만 RC4를 추가했다. 준비기·guard는 RC4의 source/JAR hash·크기, helper/PropertiesLauncher, Native catalog lock, keytool을 대조한다.
- 제품 lock이 없는 `validate_preparation`/`assert_preparation_unchanged`를 추가했다. RC4는 명시 40자리 revision, 준비 scope, 정확한 새 RocksDB data 경로와 data 부재, 자격증명/공개자료/거래 검사를 요구한다. runtime guard와 runner는 RC4를 거절한다.
- 계약4파일과 기존 BXDL 관리 bootstrap/health 소비 경계는 동일하므로 Rust parser 허용 범위를 변경하지 않았다. Native 공급 내용·Console projection 변경과 후속 유한 통합 사례를 별도 설계에 기록했다.

## 실제 준비와 독립 검증

새 private 경로에서 repository 준비기의 CLI/main 경로를1회 실행했다. 관측 wrapper는 subprocess 호출 정보를 기록했으며 생성기의 인자나 바이트를 바꾸지 않았다.

| 검증 | 실제 결과 |
| --- | --- |
| 직접 준비 child | helper Java1 + offline Node signer1 + keytool4 = **6회**, 모두 exit0·자동 재시도0 |
| 준비 대상 | validator4·observer0, chain11578/DEV_QBFT, loopback HTTP/P2P 분리 |
| 새 자격증명 | 4노드 ×6파일 =24개, regular·nlink1·0600·비어 있지 않음 |
| 공개 인증서 | DER4개의 NIGO domain hash를 독립 재계산해 manifest pin과 일치 |
| 거래 | 새 offline wallet의 Type-2 nonce0~11 총12개. 준비기가 암호학적 decode·sender/chain/type/hash를 확인; 제출0회 |
| 준비 guard | 공개 입력8개·거래12개 검증, 재검사 무변경, `engineLockSha256=null` |
| runtime 차단 | `FIXTURE_GUARD_RUNTIME_CANDIDATE_NOT_SUPPORTED`, 실행 지원으로 승격하지 않음 |
| 입력 불변 | JRE259파일·JAR/Node/실행 입력, 선언 dependencies9패키지·1,961파일 전후 일치 |
| 정리 | 정상 종료·timeout 없음·정리 신호0·소유 group 부재, HTTP/P2P8포트 재바인딩 가능 |
| data | 빈 data 상위 폴더만 존재, 노드 data·genesis DB 미생성·서비스/노드 기동0 |

controller 관측 시간은 약 2.657초이며 SLA가 아니다. 240초는 driver 대기 한도이고 전후 inventory·종료 유예는 별도다. clean env의 HOME/TMPDIR와 private cwd를 제공했으나 JVM system property 고정·OS 네트워크 차단·실제 loaded dependency 전체 계측을 주장하지 않는다. 키 내용과 raw 거래는 Git에 넣지 않는다.

준비기 자체의 Native 검사는 `LOCK_BYTES_AND_SUMMARY_ONLY`다. 별도 정적 검증으로 수신 JAR의 nested VM·lock/sidecar와 prefix380개, payload378파일의 전체 membership/hash/size·합계9,017,808bytes도 대조했다. [구조화 결과](./2026-11-15-rc4-fixture-preparation.json)의 `nativeCatalog`에 그 독립 근거를 남겼다. Java catalog loader·Native 실행·실제 genesis 검증은 아니다.

## 자동 검사와 남은 범위

`make check` PASS: fmt·Clippy, Python **114개 중112 PASS·2 SKIP**, Rust **592 PASS**, 실패0. 후보 회귀46개는 Python 집계에 포함된다. SKIP2개는 별도 실제 CLI 배포·실제 바이너리 ZLE 인수 입력이 필요한 기존 opt-in 사례다. 이 두 과업을 RC4-02의 준비 성공으로 완료 처리하지 않는다.

회귀는 기존RC1~3, RC4 준비/실행 경계, 누락·잘못된 helper/catalog, 후보/source/size 혼용, fresh data·참조 경로·명시 revision, 준비 증거 변경을 포함한다. 실제 시험 자료와 코드·증거를 독립 검토했다.

다음은 **RC4-03 내부 실행기 고정·RC.4 lock/spec 명시 선택·runtime 조립**이다. RC4-04의 외부 관리 CLI profile, RC4-05/06의 init·서비스·동종4노드 합의/복구, 정식 제품 회신과 사람 UX 과업은 별도다. helper 사용이 확인돼 현재 추가 NIGO 선행 구현 요청은 없다. RC.3 DB migration·혼합 버전 rolling·Linux/H2/Docker·고객 납품 승인으로 범위를 넓히지 않는다.
