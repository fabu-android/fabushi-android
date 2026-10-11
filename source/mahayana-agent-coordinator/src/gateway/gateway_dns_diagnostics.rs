#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsProbeResult { Resolved, Timeout, NotFound, TemporaryFailure, Error }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DnsDiagnosis {
    ResolvedBeforeProbe, SystemPathFailure, IndependentPathFailure, EndpointFailure,
    ClusterFailure, GeneralDnsFailure, Inconclusive,
}

pub fn classify_dns_diagnosis(system: DnsProbeResult, exact: DnsProbeResult, wildcard: DnsProbeResult, general: DnsProbeResult) -> DnsDiagnosis {
    use DnsProbeResult::Resolved;
    if system == Resolved && exact == Resolved { DnsDiagnosis::ResolvedBeforeProbe }
    else if system != Resolved && exact == Resolved { DnsDiagnosis::SystemPathFailure }
    else if system == Resolved && exact != Resolved { DnsDiagnosis::IndependentPathFailure }
    else if exact != Resolved && wildcard == Resolved { DnsDiagnosis::EndpointFailure }
    else if exact != Resolved && wildcard != Resolved && general == Resolved { DnsDiagnosis::ClusterFailure }
    else if general != Resolved { DnsDiagnosis::GeneralDnsFailure }
    else { DnsDiagnosis::Inconclusive }
}
